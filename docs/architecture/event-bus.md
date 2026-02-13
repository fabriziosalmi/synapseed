# Event Bus

SYNAPSEED uses an async broadcast channel for inter-plugin communication. Events flow through the system reactively, enabling loose coupling between subsystems.

## SynapseContext

The `SynapseContext` is the central shared state object, holding:

- **Project root path** — The filesystem root of the analyzed project
- **Project state** — Detected state (Virgin, Partial, Healthy, Unknown)
- **DNA configuration** — Loaded from `.synapseed/dna.yaml`
- **Broadcast channel** — `tokio::sync::broadcast` with 4096-event capacity
- **Metrics** — Thread-safe counters for indexing, DLP, commands, events
- **Extensions** — Type-erased `HashMap<TypeId, Arc<dyn Any>>` for cross-crate sharing

```rust
// Broadcasting an event
let receivers = ctx.broadcast(SynapseEvent::FileChanged {
    path: "src/main.rs".into(),
    kind: FileChangeKind::Modified,
});

// Subscribing to events
let mut rx = ctx.subscribe();
loop {
    match rx.recv().await {
        Ok(event) => handle(event),
        Err(RecvError::Lagged(n)) => warn!("Missed {n} events"),
        Err(RecvError::Closed) => break,
    }
}
```

## Event Types

```rust
pub enum SynapseEvent {
    SystemInit { project_root, state },
    FileChanged { path, kind },
    SymbolResolved { name, file, line },
    SecurityAlert { rule, severity, context },
    CommandEvaluated { command, allowed },
    ScaffoldRequested { template },
    GitStateChanged { head, branch },
    DiagnosticUpdated { errors, warnings },
    TelemetryUpdate { spans_received, hotspot_file, hotspot_duration_ms },
    IndexingComplete,
    SystemShutdown,
}
```

## Event Flow Examples

### File Watcher Pipeline

```
File modified on disk
  → notify crate detects change
  → VisualizerPlugin broadcasts FileChanged
  → WebSocket pushes event to browser
  → Cytoscape.js pulses the corresponding node
```

### Telemetry Pipeline

```
External app sends OTLP trace
  → TelemetrySink receives gRPC request
  → Resolves span to source symbol via Cortex
  → Pushes to SpanStore ring buffer
  → Broadcasts TelemetryUpdate event
  → Visualizer refreshes heatmap colors
```

### Indexing Pipeline

```
MCP server starts
  → CortexPlugin begins background indexing
  → AST parsing completes for all project files
  → Broadcasts IndexingComplete event
  → Search index becomes available
```

### Shadow Compiler Pipeline

```
File modified on disk
  → ShadowCheckPlugin detects change
  → Runs `cargo check --message-format=json`
  → Parses diagnostics into DiagnosticStore
  → Broadcasts DiagnosticUpdated event
```

## Extension Registry

The `SynapseContext` includes a type-erased extension registry (`HashMap<TypeId, Arc<dyn Any>>`) for cross-crate data sharing without compile-time coupling.

Used by:
- **MomentumEngine** — Registered during MCP `initialize`, accessed by Whisper for tier/phase detection
- **DiagnosticStore** — Shared compiler diagnostics
- **SpanStore** — Telemetry span ring buffer
