//! OTLP gRPC TraceService implementation.
//!
//! Receives traces on port 4317, extracts code location attributes,
//! resolves symbols via Cortex, and pushes to the SpanStore.
//!
//! This entire module is gated behind the `grpc` feature. Without it,
//! the telemetry-sink crate still provides the in-memory [`SpanStore`]
//! and a no-op plugin stub — but skips the 35+ tonic/gRPC dependencies.

use std::net::SocketAddr;

use opentelemetry_proto::tonic::collector::trace::v1::{
    trace_service_server::{TraceService, TraceServiceServer},
    ExportTraceServiceRequest, ExportTraceServiceResponse,
};
use tonic::{Request, Response, Status};
use tracing::{debug, info};

use synapseed_core::context::SynapseContext;
use synapseed_core::event::SynapseEvent;
use synapseed_cortex::graph::CodeGraph;
use synapseed_cortex::parser::AstParser;

use crate::store::{ResolvedSpan, SpanStore};

/// The OTLP TraceService gRPC handler.
pub struct OtlpTraceService {
    store: SpanStore,
    ctx: SynapseContext,
}

impl OtlpTraceService {
    pub fn new(store: SpanStore, ctx: SynapseContext) -> Self {
        Self { store, ctx }
    }
}

#[tonic::async_trait]
impl TraceService for OtlpTraceService {
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> Result<Response<ExportTraceServiceResponse>, Status> {
        let req = request.into_inner();
        let mut total_spans = 0usize;
        let mut hotspot_file: Option<String> = None;
        let mut hotspot_duration: f64 = 0.0;

        for resource_spans in &req.resource_spans {
            // Extract service name from resource attributes
            let service_name = resource_spans
                .resource
                .as_ref()
                .map(|r| {
                    r.attributes
                        .iter()
                        .find(|kv| kv.key == "service.name")
                        .and_then(|kv| {
                            kv.value
                                .as_ref()
                                .and_then(|v| v.value.as_ref())
                                .and_then(|val| {
                                    if let opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(s) = val {
                                        Some(s.clone())
                                    } else {
                                        None
                                    }
                                })
                        })
                        .unwrap_or_default()
                })
                .unwrap_or_default();

            for scope_spans in &resource_spans.scope_spans {
                for span in &scope_spans.spans {
                    total_spans += 1;

                    // Extract code location from span attributes
                    let mut file_path: Option<String> = None;
                    let mut line_number: Option<u32> = None;

                    for attr in &span.attributes {
                        match attr.key.as_str() {
                            "code.file.path" | "code.filepath" => {
                                if let Some(ref v) = attr.value {
                                    if let Some(opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(s)) = &v.value {
                                        file_path = Some(s.clone());
                                    }
                                }
                            }
                            "code.line.number" | "code.lineno" => {
                                if let Some(ref v) = attr.value {
                                    if let Some(opentelemetry_proto::tonic::common::v1::any_value::Value::IntValue(n)) = &v.value {
                                        line_number = Some(*n as u32);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }

                    // Calculate duration (nanoseconds → milliseconds)
                    let duration_ms = if span.end_time_unix_nano > span.start_time_unix_nano {
                        (span.end_time_unix_nano - span.start_time_unix_nano) as f64 / 1_000_000.0
                    } else {
                        0.0
                    };

                    // Try to resolve symbol name via Cortex
                    let symbol_name = resolve_symbol(&file_path, line_number, &self.ctx);

                    // Track hottest span in this batch
                    if duration_ms > hotspot_duration {
                        hotspot_duration = duration_ms;
                        hotspot_file = file_path.clone();
                    }

                    let trace_id = bytes_to_hex(&span.trace_id);
                    let span_id = bytes_to_hex(&span.span_id);

                    let resolved = ResolvedSpan {
                        trace_id,
                        span_id,
                        operation_name: span.name.clone(),
                        service_name: service_name.clone(),
                        duration_ms,
                        file_path,
                        line_number,
                        symbol_name,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    };

                    self.store.push(resolved);
                }
            }
        }

        if total_spans > 0 {
            debug!(spans = total_spans, "Telemetry: Ingested OTLP trace batch");

            self.ctx.broadcast(SynapseEvent::TelemetryUpdate {
                spans_received: total_spans,
                hotspot_file,
                hotspot_duration_ms: if hotspot_duration > 0.0 {
                    Some(hotspot_duration)
                } else {
                    None
                },
            });
        }

        Ok(Response::new(ExportTraceServiceResponse {
            partial_success: None,
        }))
    }
}

/// Try to resolve a file path + line number to a symbol name via Cortex.
fn resolve_symbol(
    file_path: &Option<String>,
    line_number: Option<u32>,
    ctx: &SynapseContext,
) -> Option<String> {
    let file = file_path.as_deref()?;
    let line = line_number? as usize;

    let root = ctx.project_root();
    let full_path = root.join(file);
    if !full_path.exists() {
        return None;
    }

    let mut parser = AstParser::new().ok()?;
    let graph = CodeGraph::new();
    let _ = graph.index_file(&mut parser, &full_path, file);

    // Find the symbol containing this line
    for file_info in graph.all_files() {
        for sym in &file_info.symbols {
            if line >= sym.line_start && line <= sym.line_end {
                return Some(sym.name.clone());
            }
        }
    }

    None
}

/// Convert bytes to hex string (avoids adding `hex` crate dependency).
fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Start the OTLP gRPC server.
pub async fn start(
    addr: SocketAddr,
    store: SpanStore,
    ctx: SynapseContext,
) -> Result<(), tonic::transport::Error> {
    let token = ctx.shutdown_token();
    let service = OtlpTraceService::new(store, ctx);

    info!(addr = %addr, "Telemetry: OTLP gRPC server listening");

    tonic::transport::Server::builder()
        .add_service(TraceServiceServer::new(service))
        .serve_with_shutdown(addr, async move { token.cancelled().await })
        .await?;

    info!("Telemetry: gRPC server stopped");
    Ok(())
}
