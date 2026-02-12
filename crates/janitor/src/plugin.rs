use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use synapseed_core::context::SynapseContext;
use synapseed_core::error::Result;
use synapseed_core::event::SynapseEvent;
use synapseed_core::plugin::SynapsePlugin;
use tracing::info;

use crate::ProposalStore;

/// The JanitorPlugin — registers the ProposalStore as a shared extension
/// so MCP tools (`janitor_run_now`, `janitor_apply_fix`) can access it.
pub struct JanitorPlugin {
    store: Arc<ProposalStore>,
}

impl JanitorPlugin {
    pub fn new() -> Self {
        Self {
            store: Arc::new(ProposalStore::new()),
        }
    }

    pub fn store(&self) -> &Arc<ProposalStore> {
        &self.store
    }
}

impl Default for JanitorPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl SynapsePlugin for JanitorPlugin {
    fn name(&self) -> &str {
        "janitor"
    }

    fn on_init(&mut self, ctx: &SynapseContext) -> Result<()> {
        ctx.set_extension(self.store.clone());
        info!("Janitor plugin initialized — autonomous maintenance ready");
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
        250 // Very low priority — utility/maintenance plugin
    }
}
