use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Check if self-telemetry is enabled via `SYNAPSEED_SELF_TELEMETRY=1`.
#[cfg(feature = "self-telemetry")]
fn self_telemetry_enabled() -> bool {
    std::env::var("SYNAPSEED_SELF_TELEMETRY").ok().as_deref() == Some("1")
}

/// OTLP gRPC endpoint of the TelemetrySink receiver.
#[cfg(feature = "self-telemetry")]
const SELF_TELEMETRY_ENDPOINT: &str = "http://localhost:4317";

/// Build the tracer provider that exports Synapseed's own spans.
///
/// OTLP over gRPC (tonic) to `endpoint`, batched by a BatchSpanProcessor.
/// The resource carries only `service.name = synapseed-internal`.
///
/// Returns None if the exporter cannot be built.
#[cfg(feature = "self-telemetry")]
fn build_tracer_provider(endpoint: &str) -> Option<opentelemetry_sdk::trace::SdkTracerProvider> {
    use opentelemetry_otlp::WithExportConfig;

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .ok()?;

    // builder_empty(): no SDK or environment detectors, so the resource holds
    // exactly the attributes listed here.
    let resource = opentelemetry_sdk::Resource::builder_empty()
        .with_attributes([opentelemetry::KeyValue::new(
            "service.name",
            "synapseed-internal",
        )])
        .build();

    Some(
        opentelemetry_sdk::trace::SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(resource)
            .build(),
    )
}

/// Build the optional OTLP tracing layer for self-instrumentation.
///
/// When `SYNAPSEED_SELF_TELEMETRY=1`, sends Synapseed's own tracing spans
/// to localhost:4317 via gRPC OTLP using a BatchSpanProcessor.
/// Service name: `synapseed-internal`.
///
/// Returns None if disabled or if setup fails (graceful degradation).
#[cfg(feature = "self-telemetry")]
fn build_otel_layer<S>() -> Option<Box<dyn tracing_subscriber::Layer<S> + Send + Sync + 'static>>
where
    S: tracing::Subscriber
        + for<'span> tracing_subscriber::registry::LookupSpan<'span>
        + Send
        + Sync,
{
    if !self_telemetry_enabled() {
        return None;
    }

    use opentelemetry::trace::TracerProvider as _;

    let provider = build_tracer_provider(SELF_TELEMETRY_ENDPOINT)?;
    let tracer = provider.tracer("synapseed");

    // Store provider globally to keep the batch processor alive
    opentelemetry::global::set_tracer_provider(provider);

    Some(Box::new(tracing_opentelemetry::layer().with_tracer(tracer)))
}

/// Stub: when `self-telemetry` feature is disabled, always returns None.
#[cfg(not(feature = "self-telemetry"))]
fn build_otel_layer<S>() -> Option<Box<dyn tracing_subscriber::Layer<S> + Send + Sync + 'static>>
where
    S: tracing::Subscriber
        + for<'span> tracing_subscriber::registry::LookupSpan<'span>
        + Send
        + Sync,
{
    None
}

/// Initialize structured logging for SYNAPSEED.
///
/// Output format is controlled by `SYNAPSEED_LOG_FORMAT`:
/// - `"json"` — Machine-readable JSON lines (for production/ingestion)
/// - anything else — Human-readable compact format (default for dev)
///
/// Log level is controlled by `RUST_LOG` env var (default: `info`).
///
/// When `SYNAPSEED_SELF_TELEMETRY=1`, an OTLP exporter layer sends
/// Synapseed's own spans to localhost:4317 (the TelemetrySink receiver).
pub fn init_telemetry() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let format = std::env::var("SYNAPSEED_LOG_FORMAT").unwrap_or_default();

    if format == "json" {
        tracing_subscriber::registry()
            .with(filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(std::io::stderr)
                    .json(),
            )
            .with(build_otel_layer())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(std::io::stderr)
                    .compact(),
            )
            .with(build_otel_layer())
            .init();
    }
}

/// Initialize telemetry with ALL output forced to stderr.
///
/// This is critical for MCP server mode where stdout is the JSON-RPC
/// transport. Any stray output on stdout would corrupt the protocol.
///
/// When `SYNAPSEED_SELF_TELEMETRY=1`, an OTLP exporter layer sends
/// Synapseed's own spans to localhost:4317 (the TelemetrySink receiver).
pub fn init_telemetry_stderr() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .compact(),
        )
        .with(build_otel_layer())
        .init();
}

#[cfg(all(test, feature = "self-telemetry"))]
mod tests {
    use std::sync::{Arc, Mutex};

    use opentelemetry::trace::{Tracer as _, TracerProvider as _};
    use opentelemetry_proto::tonic::collector::trace::v1::{
        trace_service_server::{TraceService, TraceServiceServer},
        ExportTraceServiceRequest, ExportTraceServiceResponse,
    };
    use opentelemetry_proto::tonic::common::v1::any_value::Value;

    use super::build_tracer_provider;

    /// Minimal OTLP trace receiver that records every export request.
    #[derive(Clone, Default)]
    struct Collector {
        requests: Arc<Mutex<Vec<ExportTraceServiceRequest>>>,
    }

    #[tonic::async_trait]
    impl TraceService for Collector {
        async fn export(
            &self,
            request: tonic::Request<ExportTraceServiceRequest>,
        ) -> Result<tonic::Response<ExportTraceServiceResponse>, tonic::Status> {
            self.requests.lock().unwrap().push(request.into_inner());
            Ok(tonic::Response::new(ExportTraceServiceResponse::default()))
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn self_telemetry_exports_flushes_and_shuts_down() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let collector = Collector::default();
        let server = tokio::spawn(
            tonic::transport::Server::builder()
                .add_service(TraceServiceServer::new(collector.clone()))
                .serve_with_incoming(tonic::transport::server::TcpIncoming::from(listener)),
        );

        let provider =
            build_tracer_provider(&format!("http://{addr}")).expect("OTLP exporter builds");
        provider
            .tracer("synapseed-test")
            .in_span("self-telemetry-test-span", |_| {});

        // force_flush and shutdown wait on the batch processor, so keep them
        // off the async workers.
        let p = provider.clone();
        tokio::task::spawn_blocking(move || {
            p.force_flush().expect("force_flush succeeds");
            p.shutdown().expect("shutdown succeeds");
        })
        .await
        .unwrap();
        server.abort();

        let requests = collector.requests.lock().unwrap();
        let resource_spans: Vec<_> = requests.iter().flat_map(|r| &r.resource_spans).collect();
        let span_names: Vec<&str> = resource_spans
            .iter()
            .flat_map(|rs| &rs.scope_spans)
            .flat_map(|ss| &ss.spans)
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(span_names, ["self-telemetry-test-span"]);

        // The resource holds exactly service.name, as it did on 0.27.
        for rs in &resource_spans {
            let attrs: Vec<(&str, Option<&Value>)> = rs
                .resource
                .as_ref()
                .expect("resource is sent")
                .attributes
                .iter()
                .map(|kv| {
                    (
                        kv.key.as_str(),
                        kv.value.as_ref().and_then(|v| v.value.as_ref()),
                    )
                })
                .collect();
            assert_eq!(
                attrs,
                [(
                    "service.name",
                    Some(&Value::StringValue("synapseed-internal".into()))
                )]
            );
        }

        // A second shutdown reports that the provider is already shut down.
        assert!(provider.shutdown().is_err());
    }
}
