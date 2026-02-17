use std::future::Future;
use std::pin::Pin;

use synapseed_core::context::SynapseContext;
use synapseed_core::error::Result;
use synapseed_core::event::SynapseEvent;
use synapseed_core::plugin::SynapsePlugin;
use tracing::info;

use crate::Trainer;

/// The GymPlugin — registers the Trainer as a shared extension
/// so the MCP tool `train_code` can use it.
pub struct GymPlugin {
    trainer: Trainer,
}

impl GymPlugin {
    pub fn new() -> Self {
        Self {
            trainer: Trainer::new(),
        }
    }

    pub fn trainer(&self) -> &Trainer {
        &self.trainer
    }
}

impl Default for GymPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl SynapsePlugin for GymPlugin {
    fn name(&self) -> &str {
        "gym"
    }

    fn on_init(&mut self, _ctx: &SynapseContext) -> Result<()> {
        info!("Gym plugin initialized — RL sandbox ready");
        Ok(())
    }

    fn on_event<'a>(
        &'a self,
        _event: &'a SynapseEvent,
        _ctx: &'a SynapseContext,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SynapseEvent>>> + Send + 'a>> {
        Box::pin(async { Ok(None) })
    }

    fn priority(&self) -> u32 {
        200 // Low priority — utility plugin
    }
}
