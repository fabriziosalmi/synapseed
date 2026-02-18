//! Shadow Check Plugin — background compiler diagnostics.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tracing::{info, warn};

use synapseed_core::context::SynapseContext;
use synapseed_core::error::Result;
use synapseed_core::event::{FileChangeKind, SynapseEvent};
use synapseed_core::plugin::SynapsePlugin;

use crate::runner::{self, DiagnosticStore};

/// The Shadow Check plugin — runs `cargo check` in background.
pub struct ShadowCheckPlugin {
    trigger_tx: Option<std::sync::mpsc::Sender<()>>,
}

impl ShadowCheckPlugin {
    pub fn new() -> Self {
        Self { trigger_tx: None }
    }
}

impl Default for ShadowCheckPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl SynapsePlugin for ShadowCheckPlugin {
    fn name(&self) -> &str {
        "shadow-check"
    }

    fn on_init(&mut self, ctx: &SynapseContext) -> Result<()> {
        let root = ctx.project_root();

        // D39: Respect DNA opt-out — disable for untrusted projects where
        // build.rs scripts may execute arbitrary code with user privileges.
        if !ctx.dna().hci.shadow_check {
            info!("Shadow: Disabled via dna.yaml (hci.shadow_check: false)");
            return Ok(());
        }

        // Only activate for Rust projects (must have Cargo.toml)
        if !root.join("Cargo.toml").exists() {
            info!("Shadow: No Cargo.toml found, skipping");
            return Ok(());
        }

        let store = Arc::new(DiagnosticStore::new(root));

        // Register in context for MCP tool access
        ctx.set_extension(store.clone());

        // Create the trigger channel
        let (tx, rx) = std::sync::mpsc::channel();
        self.trigger_tx = Some(tx);

        // Start the background loop
        runner::start_background_loop(store, ctx.clone(), rx);

        info!("Shadow: Background compiler active");
        Ok(())
    }

    fn on_event<'a>(
        &'a self,
        event: &'a SynapseEvent,
        _ctx: &'a SynapseContext,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SynapseEvent>>> + Send + 'a>> {
        Box::pin(async move {
            if let SynapseEvent::FileChanged { path, kind } = event {
                // Trigger a recheck on source file changes.
                // v4.17.1 (W5): Also recheck on Deleted — stale diagnostics
                // for removed files persist until the next recheck.
                if matches!(
                    kind,
                    FileChangeKind::Created | FileChangeKind::Modified | FileChangeKind::Deleted
                ) {
                    let is_source = path.ends_with(".rs")
                        || path.ends_with(".toml")
                        || path.ends_with(".py")
                        || path.ends_with(".js");

                    if is_source {
                        if let Some(tx) = &self.trigger_tx {
                            if let Err(e) = tx.send(()) {
                                warn!(error = %e, "ShadowCheck: Failed to send recheck trigger");
                            }
                        }
                    }
                }
            }

            Ok(None)
        })
    }

    fn priority(&self) -> u32 {
        150 // After cortex (100), before search (200)
    }
}
