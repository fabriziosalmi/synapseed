use std::future::Future;
use std::pin::Pin;

use crate::context::SynapseContext;
use crate::error::Result;
use crate::event::SynapseEvent;

/// The Plugin trait — the extension point for all SYNAPSEED modules.
///
/// Cortex, Husk, Root, and Chronos all implement this trait.
/// Plugins receive lifecycle events and can interact with the
/// shared SynapseContext.
///
/// # Lifecycle
/// 1. `on_init` — Called once at startup. Load config, warm caches.
/// 2. `on_event` — Called for every domain event. React accordingly.
/// 3. `on_shutdown` — Called once at teardown. Flush state, cleanup.
///
/// # Event Bus
/// The SynapseContext contains an async broadcast channel.
/// When `ctx.broadcast(event)` is called, all registered plugins
/// receive the event via `on_event` in parallel.
pub trait SynapsePlugin: Send + Sync {
    /// Human-readable name for logging and diagnostics.
    fn name(&self) -> &str;

    /// Called once during system initialization.
    fn on_init(&mut self, ctx: &SynapseContext) -> Result<()>;

    /// Called for each domain event broadcast through the bus.
    /// Return Ok(None) to consume silently, or Ok(Some(event))
    /// to emit a new event downstream (event chaining).
    fn on_event<'a>(
        &'a self,
        event: &'a SynapseEvent,
        ctx: &'a SynapseContext,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SynapseEvent>>> + Send + 'a>>;

    /// Called once during graceful shutdown.
    fn on_shutdown(&self, _ctx: &SynapseContext) -> Result<()> {
        Ok(())
    }

    /// Priority for event processing order (lower = earlier).
    /// Security plugins should use low numbers (high priority).
    fn priority(&self) -> u32 {
        100
    }
}
