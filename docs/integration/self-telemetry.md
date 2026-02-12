# Self-Telemetry

SYNAPSEED can observe its own performance through a **dogfooding loop**: it sends its own tracing spans to its own OTLP receiver.

## How It Works

```
SYNAPSEED operation (e.g., scan_security)
  → tracing emits a span
  → BatchSpanProcessor buffers the span
  → gRPC export to localhost:4317
  → TelemetrySink receives the span
  → SpanStore records metrics
  → Visualizer colors the node RED if slow
```

## Enabling Self-Telemetry

Set the `SYNAPSEED_SELF_TELEMETRY` environment variable:

```bash
SYNAPSEED_SELF_TELEMETRY=1 synapseed serve --project .
```

Or in your MCP configuration:

```json
{
  "mcpServers": {
    "synapseed": {
      "command": "synapseed",
      "args": ["serve", "--project", "."],
      "env": {
        "SYNAPSEED_SELF_TELEMETRY": "1"
      }
    }
  }
}
```

## Technical Details

- **Service name:** `synapseed-internal`
- **Endpoint:** `http://localhost:4317` (same port as the TelemetrySink receiver)
- **Processor:** `BatchSpanProcessor` (async, non-blocking)
- **Runtime:** Tokio (via `opentelemetry_sdk` `rt-tokio` feature)

## Boot Order

1. `init_telemetry_stderr()` initializes the tracing subscriber with the OTLP layer. The `BatchSpanProcessor` buffers spans in memory.
2. Plugins initialize in priority order. `TelemetrySinkPlugin` (priority 200) starts the gRPC server on port 4317.
3. Once the server is ready, buffered spans are delivered automatically.

The `BatchSpanProcessor` handles the race condition gracefully — if the server isn't ready, spans are buffered and retried.

## Viewing Results

1. Open the Visualizer at `http://localhost:3000`
2. Trigger some operations (e.g., ask Claude to scan code)
3. Observe the heatmap coloring on the code graph
4. Read `synapseed://telemetry/hotspots` for detailed metrics

## Dependencies

The self-instrumentation adds these crates to `synapseed-core`:

| Crate | Version | Purpose |
| :--- | :--- | :--- |
| `opentelemetry` | 0.27 | Core traits and types |
| `opentelemetry_sdk` | 0.27 | TracerProvider and BatchSpanProcessor |
| `opentelemetry-otlp` | 0.27 | gRPC OTLP exporter (tonic) |
| `tracing-opentelemetry` | 0.28 | Bridge between tracing and OpenTelemetry |
