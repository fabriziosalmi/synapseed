use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Check if self-telemetry is enabled via `SYNAPSEED_SELF_TELEMETRY=1`.
fn self_telemetry_enabled() -> bool {
    std::env::var("SYNAPSEED_SELF_TELEMETRY")
        .ok()
        .as_deref()
        == Some("1")
}

/// Build the optional OTLP tracing layer for self-instrumentation.
///
/// When `SYNAPSEED_SELF_TELEMETRY=1`, sends Synapseed's own tracing spans
/// to localhost:4317 via gRPC OTLP using an async BatchSpanProcessor.
/// Service name: `synapseed-internal`.
///
/// Returns None if disabled or if setup fails (graceful degradation).
fn build_otel_layer<S>() -> Option<Box<dyn tracing_subscriber::Layer<S> + Send + Sync + 'static>>
where
    S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span> + Send + Sync,
{
    if !self_telemetry_enabled() {
        return None;
    }

    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_otlp::WithExportConfig;

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint("http://localhost:4317")
        .build()
        .ok()?;

    let provider = opentelemetry_sdk::trace::TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_resource(opentelemetry_sdk::Resource::new(vec![
            opentelemetry::KeyValue::new("service.name", "synapseed-internal"),
        ]))
        .build();

    let tracer = provider.tracer("synapseed");

    // Store provider globally to keep the batch processor alive
    opentelemetry::global::set_tracer_provider(provider);

    Some(Box::new(tracing_opentelemetry::layer().with_tracer(tracer)))
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
            .with(tracing_subscriber::fmt::layer().json())
            .with(build_otel_layer())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer().compact())
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
