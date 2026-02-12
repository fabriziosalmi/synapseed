//! Whisperer Plugin — Intent-Aware Orchestrator.
//!
//! Registers last (highest priority number) so all other subsystems
//! are available when the Whisperer executes a plan.

use std::future::Future;
use std::pin::Pin;

use synapseed_core::context::SynapseContext;
use synapseed_core::error::Result;
use synapseed_core::event::SynapseEvent;
use synapseed_core::plugin::SynapsePlugin;
use tracing::info;

/// The Whisperer plugin — intent routing and orchestration.
pub struct WhisperPlugin;

impl WhisperPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WhisperPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl SynapsePlugin for WhisperPlugin {
    fn name(&self) -> &str {
        "whisper"
    }

    fn on_init(&mut self, _ctx: &SynapseContext) -> Result<()> {
        info!("Whisperer: Intent router active (level 0 — deterministic heuristics)");
        Ok(())
    }

    fn on_event<'a>(
        &'a self,
        _event: &'a SynapseEvent,
        _ctx: &'a SynapseContext,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SynapseEvent>>> + Send + 'a>> {
        Box::pin(async move { Ok(None) })
    }

    fn priority(&self) -> u32 {
        999 // Last — all other subsystems must be ready first
    }
}
