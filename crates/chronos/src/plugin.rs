use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use synapseed_core::context::SynapseContext;
use synapseed_core::error::Result;
use synapseed_core::event::SynapseEvent;
use synapseed_core::plugin::SynapsePlugin;
use tracing::{debug, info};

use crate::historian::Historian;

/// The Chronos plugin — Git time-travel intelligence.
pub struct ChronosPlugin {
    historian: Option<Arc<Historian>>,
}

impl ChronosPlugin {
    pub fn new() -> Self {
        Self { historian: None }
    }

    pub fn historian(&self) -> Option<&Historian> {
        self.historian.as_deref()
    }
}

impl Default for ChronosPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl SynapsePlugin for ChronosPlugin {
    fn name(&self) -> &str {
        "chronos"
    }

    fn on_init(&mut self, ctx: &SynapseContext) -> Result<()> {
        let root = ctx.project_root();
        match Historian::open(&root) {
            Ok(historian) => {
                let summary = historian.summary(5)?;
                info!(
                    head = %summary.head_commit,
                    branch = ?summary.branch,
                    commits = summary.total_commits,
                    dirty = summary.is_dirty,
                    "Chronos: Git history loaded"
                );
                let shared = Arc::new(historian);
                // Register as extension so MCP tools reuse the same Historian
                ctx.set_extension(shared.clone());
                self.historian = Some(shared);
            }
            Err(e) => {
                info!(error = %e, "Chronos: No git repository found, running without history");
            }
        }
        Ok(())
    }

    fn on_event<'a>(
        &'a self,
        event: &'a SynapseEvent,
        _ctx: &'a SynapseContext,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SynapseEvent>>> + Send + 'a>> {
        Box::pin(async move {
            match event {
                SynapseEvent::FileChanged { path, .. } => {
                    if let Some(historian) = &self.historian {
                        if let Ok(blame) = historian.blame_lines(path, 1, 10) {
                            if !blame.is_empty() {
                                info!(
                                    file = %path,
                                    last_author = %blame[0].author,
                                    last_commit = %blame[0].commit_id,
                                    "Chronos: File history context"
                                );
                            }
                        }
                    }
                    Ok(None)
                }
                SynapseEvent::GitStateChanged { head, branch } => {
                    debug!(
                        head = %head,
                        branch = ?branch,
                        "Chronos: Git state changed, history cache implicitly refreshed"
                    );
                    // Historian reads live from the repo (no stale cache) —
                    // git2 operations always reflect current disk state.
                    // Log the event for observability.
                    Ok(None)
                }
                _ => Ok(None),
            }
        })
    }

    fn priority(&self) -> u32 {
        200
    }
}
