use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use synapseed_core::context::SynapseContext;
use synapseed_core::error::Result;
use synapseed_core::event::SynapseEvent;
use synapseed_core::plugin::SynapsePlugin;
use tracing::info;

use crate::sentinel::Sentinel;

/// The Root plugin — command execution sandbox.
pub struct RootPlugin {
    sentinel: Option<Arc<Sentinel>>,
}

impl RootPlugin {
    pub fn new() -> Self {
        Self { sentinel: None }
    }

    pub fn sentinel(&self) -> Option<&Sentinel> {
        self.sentinel.as_deref()
    }
}

impl Default for RootPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl SynapsePlugin for RootPlugin {
    fn name(&self) -> &str {
        "root"
    }

    fn on_init(&mut self, ctx: &SynapseContext) -> Result<()> {
        let sentinel = Arc::new(Sentinel::with_defaults()?);
        ctx.set_extension(sentinel.clone());
        self.sentinel = Some(sentinel);
        info!("Root: Command sentinel active (fail-closed)");
        Ok(())
    }

    fn on_event<'a>(
        &'a self,
        event: &'a SynapseEvent,
        ctx: &'a SynapseContext,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SynapseEvent>>> + Send + 'a>> {
        Box::pin(async move {
            match event {
                SynapseEvent::CommandEvaluated { command, allowed } => {
                    if *allowed {
                        ctx.update_metrics(|m| m.commands_allowed += 1);
                    } else {
                        ctx.update_metrics(|m| m.commands_denied += 1);
                    }
                    info!(
                        command = %command,
                        allowed = allowed,
                        "Root: Command evaluation recorded"
                    );
                    Ok(None)
                }
                _ => Ok(None),
            }
        })
    }

    fn priority(&self) -> u32 {
        20 // High priority — command safety is critical
    }
}
