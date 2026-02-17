# Telemetry — OTLP Receiver

The **Telemetry Sink** transforms SYNAPSEED from a static analysis tool into a **live runtime intelligence hub**. It receives OpenTelemetry traces from running applications and maps them back to source code.

## Architecture

```
Instrumented App (OpenTelemetry SDK)
  → gRPC :4317 (OTLP TraceService)
    → TelemetrySink (parse spans)
      → Extract code.file.path + code.line.number
        → Cortex (resolve file:line → Symbol)
          → SpanStore (ring buffer, 1000 spans)
            → Broadcast TelemetryUpdate event
              → Available via synapseed://telemetry/hotspots
```

## How It Works

1. Your application is instrumented with OpenTelemetry and sends traces to `localhost:4317`
2. SYNAPSEED's gRPC server receives `ExportTraceServiceRequest` messages
3. For each span, it extracts the `code.file.path` and `code.line.number` attributes
4. It resolves these to source symbols via the Cortex AST engine
5. Spans are stored in a ring buffer (max 1000) with per-symbol metrics
6. Hotspot data is available via the `synapseed://telemetry/hotspots` resource

## SpanStore

The ring buffer stores:

- **ResolvedSpan** — Individual span data (trace ID, operation, duration, file, symbol)
- **SpanMetrics** — Per-symbol aggregates (call count, avg/max/p95 duration)

### Hotspot Classification

| Duration | Level |
| :--- | :--- |
| > 200ms | Hot |
| 50–200ms | Warm |
| < 50ms | Cool |

## Instrumenting Your App

Add OpenTelemetry to your application and configure the OTLP exporter:

```rust
// Rust example
let exporter = opentelemetry_otlp::SpanExporter::builder()
    .with_tonic()
    .with_endpoint("http://localhost:4317")
    .build()?;
```

Use the `code.file.path` and `code.line.number` span attributes for symbol resolution:

```rust
#[tracing::instrument(fields(
    code.file.path = file!(),
    code.line.number = line!(),
))]
fn my_function() {
    // ...
}
```

## Self-Instrumentation

SYNAPSEED can observe its own performance. Set `SYNAPSEED_SELF_TELEMETRY=1`:

```bash
SYNAPSEED_SELF_TELEMETRY=1 synapseed serve --project .
```

This enables the **dogfooding loop**: SYNAPSEED's tracing spans are sent to its own OTLP receiver via an async `BatchSpanProcessor`, allowing you to see SYNAPSEED's own hotspots via the `synapseed://telemetry/hotspots` resource.

## MCP Integration

| Type | Name | Description |
| :--- | :--- | :--- |
| Tool | `reset-telemetry` | Clear all spans and metrics |
| Resource | `synapseed://telemetry/hotspots` | Top-10 hotspots with metrics |
| Prompt | `optimize_hotspots` | Guided hotspot analysis workflow |
