# Plugin System

Every major subsystem in SYNAPSEED is a **plugin** that implements the `SynapsePlugin` trait. This enables clean separation of concerns, priority-based initialization, and graceful degradation.

## The SynapsePlugin Trait

```rust
pub trait SynapsePlugin: Send + Sync {
    /// Unique plugin name.
    fn name(&self) -> &str;

    /// Synchronous initialization. Called once at startup.
    fn on_init(&mut self, ctx: &SynapseContext) -> Result<()>;

    /// Async event handler. Returns an optional new event to broadcast.
    fn on_event<'a>(
        &'a self,
        event: &'a SynapseEvent,
        ctx: &'a SynapseContext,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SynapseEvent>>> + Send + 'a>>;

    /// Priority for initialization order (lower = earlier).
    fn priority(&self) -> u32 { 100 }

    /// Cleanup hook.
    fn on_shutdown(&self) {}
}
```

## Plugin Registry

Plugins are registered in `cmd_serve()` and sorted by priority before initialization:

```rust
let mut plugins: Vec<Box<dyn SynapsePlugin>> = vec![
    Box::new(HuskPlugin::new()),         // priority: 10
    Box::new(RootPlugin::new()),         // priority: 20
    Box::new(CortexPlugin::new()),       // priority: 50
    Box::new(ChronosPlugin::new()),      // priority: 100
    Box::new(ShadowCheckPlugin::new()),  // priority: 150
    Box::new(SearchPlugin::new()),       // priority: 160
    Box::new(TelemetrySinkPlugin::new()),// priority: 200
    Box::new(ArchitectPlugin::new()),    // priority: 210
    Box::new(WhisperPlugin::new()),      // priority: 999
    Box::new(GymPlugin::new()),          // priority: 1000
    Box::new(JanitorPlugin::new()),      // priority: 1010
];
plugins.sort_by_key(|p| p.priority());
```

## Priority Convention

| Range | Purpose | Examples |
| :--- | :--- | :--- |
| 0–49 | Security (must init first) | Husk (10), Root (20) |
| 50–99 | Core analysis | Cortex (50) |
| 100–149 | History & context | Chronos (100) |
| 150–199 | Tooling | Shadow (150), Search (160) |
| 200–299 | Infrastructure | Telemetry (200), Architect (210) |
| 900+ | Orchestration | Whisper (999) |

## Context Extensions

Plugins share state via type-erased extensions on `SynapseContext`:

```rust
// Plugin registers during on_init:
ctx.set_extension(Arc::new(my_store.clone()));

// MCP tools read later:
if let Some(store) = ctx.get_extension::<MyStore>() {
    store.query(...)
}
```

This pattern avoids circular dependencies — the MCP crate depends on the feature crate's types, not the other way around.

## Graceful Degradation

If a plugin fails to initialize, the system continues without it:

```rust
for plugin in &mut plugins {
    if let Err(e) = plugin.on_init(&ctx) {
        eprintln!("[WARN] {} failed to init: {e}", plugin.name());
    }
}
```

MCP tools that depend on a plugin check for its extension and return a helpful message if unavailable.
