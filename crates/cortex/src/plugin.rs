use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use synapseed_core::context::SynapseContext;
use synapseed_core::error::Result;
use synapseed_core::event::{FileChangeKind, SynapseEvent};
use synapseed_core::plugin::SynapsePlugin;
use tracing::{info, warn};

use crate::graph::CodeGraph;
use crate::parser::AstParser;

/// Maximum file size to re-index on incremental change (1 MB).
/// Matches the constant in `graph.rs` — oversized files are skipped.
const MAX_INCREMENTAL_SIZE: u64 = 1_048_576;

/// The Cortex plugin — semantic code understanding.
///
/// Uses background indexing (HCI Req 1: Zero-Friction Start) so the MCP
/// server becomes responsive immediately while the code graph populates
/// asynchronously.
pub struct CortexPlugin {
    parser: Option<AstParser>,
    graph: Arc<CodeGraph>,
}

impl CortexPlugin {
    pub fn new() -> Self {
        Self {
            parser: None,
            graph: Arc::new(CodeGraph::new()),
        }
    }

    pub fn graph(&self) -> &CodeGraph {
        &self.graph
    }
}

impl Default for CortexPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl SynapsePlugin for CortexPlugin {
    fn name(&self) -> &str {
        "cortex"
    }

    fn on_init(&mut self, ctx: &SynapseContext) -> Result<()> {
        let root = ctx.project_root();

        // Register the (initially empty) graph immediately so downstream
        // plugins and MCP tools can access it — they'll gracefully degrade
        // with zero files until background indexing completes.
        ctx.set_extension(self.graph.clone());

        let graph = Arc::clone(&self.graph);
        let ctx_clone = ctx.clone();
        std::thread::spawn(move || {
            let start = std::time::Instant::now();
            if let Err(e) = graph.index_directory(&root) {
                warn!(error = %e, "Cortex: Background indexing failed");
                return;
            }
            if ctx_clone.is_shutting_down() {
                return;
            }
            let elapsed = start.elapsed();
            info!(
                files = graph.file_count(),
                symbols = graph.symbol_count(),
                ms = elapsed.as_millis(),
                "Cortex: Background indexing complete"
            );
            ctx_clone.update_metrics(|m| {
                m.files_indexed = graph.file_count();
                m.symbols_found = graph.symbol_count();
            });
            ctx_clone.broadcast(SynapseEvent::IndexingComplete);
        });

        self.parser = AstParser::new().ok();
        Ok(())
    }

    fn on_event<'a>(
        &'a self,
        event: &'a SynapseEvent,
        _ctx: &'a SynapseContext,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SynapseEvent>>> + Send + 'a>> {
        Box::pin(async move {
            match event {
                SynapseEvent::FileChanged {
                    path,
                    kind: FileChangeKind::Deleted,
                } => {
                    // D14 fix: remove ghost symbols when a file is deleted/renamed.
                    let file_path = std::path::Path::new(path);
                    let removed = self.graph.remove_file(file_path);
                    if removed > 0 {
                        info!(
                            file = %path,
                            symbols = removed,
                            "Cortex: Removed deleted file from graph"
                        );
                    }
                    Ok(None)
                }
                SynapseEvent::FileChanged {
                    path,
                    kind: FileChangeKind::Modified | FileChangeKind::Created,
                } => {
                    // Incremental reindex: re-parse the changed file so the
                    // graph stays fresh without a full rebuild.
                    let file_path = std::path::Path::new(path);

                    // Size guard (mirrors graph.rs MAX_FILE_SIZE)
                    let too_large = std::fs::metadata(file_path)
                        .map(|m| m.len() > MAX_INCREMENTAL_SIZE)
                        .unwrap_or(false);
                    if too_large {
                        return Ok(None);
                    }

                    if let Ok(source) = std::fs::read_to_string(file_path) {
                        // Remove old entries first so stale symbols don't linger
                        self.graph.remove_file(file_path);

                        if let Ok(mut parser) = AstParser::new() {
                            match self.graph.index_file(&mut parser, file_path, &source) {
                                Ok(()) => {
                                    info!(
                                        file = %path,
                                        "Cortex: Incremental reindex complete"
                                    );
                                }
                                Err(e) => {
                                    warn!(file = %path, error = %e, "Cortex: Incremental reindex failed");
                                }
                            }
                        }
                    }
                    Ok(None)
                }
                _ => Ok(None),
            }
        })
    }

    fn priority(&self) -> u32 {
        50 // High priority — AST updates feed other plugins
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use synapseed_core::event::SynapseEvent;
    use synapseed_core::liquid::ProjectDna;
    use synapseed_core::state::ProjectState;

    /// Wait for `IndexingComplete` on the subscriber, with a timeout.
    async fn wait_for_indexing_complete(ctx: &SynapseContext, timeout: Duration) {
        let mut rx = ctx.subscribe();
        tokio::time::timeout(timeout, async {
            loop {
                if let Ok(SynapseEvent::IndexingComplete) = rx.recv().await {
                    break;
                }
            }
        })
        .await
        .expect("IndexingComplete not received within timeout");
    }

    #[tokio::test]
    async fn test_background_indexing_broadcasts_event() {
        let dir = tempfile::tempdir().unwrap();
        // Write a minimal Rust file so the indexer has something to parse.
        std::fs::write(dir.path().join("hello.rs"), "pub fn hello() {}").unwrap();

        let ctx = SynapseContext::new(
            dir.path().to_path_buf(),
            ProjectState::Unknown,
            ProjectDna::default(),
        );

        let mut plugin = CortexPlugin::new();
        plugin.on_init(&ctx).unwrap();

        wait_for_indexing_complete(&ctx, Duration::from_secs(10)).await;
    }

    #[tokio::test]
    async fn test_background_indexing_updates_metrics() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("lib.rs"),
            "pub struct Foo;\npub fn bar() -> Foo { Foo }\n",
        )
        .unwrap();

        let ctx = SynapseContext::new(
            dir.path().to_path_buf(),
            ProjectState::Unknown,
            ProjectDna::default(),
        );

        let mut plugin = CortexPlugin::new();
        plugin.on_init(&ctx).unwrap();

        wait_for_indexing_complete(&ctx, Duration::from_secs(10)).await;

        let metrics = ctx.metrics();
        assert!(
            metrics.files_indexed > 0,
            "Expected files_indexed > 0, got {}",
            metrics.files_indexed
        );
        assert!(
            metrics.symbols_found > 0,
            "Expected symbols_found > 0, got {}",
            metrics.symbols_found
        );
    }

    #[tokio::test]
    async fn test_background_indexing_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        // Empty directory — no files to index.

        let ctx = SynapseContext::new(
            dir.path().to_path_buf(),
            ProjectState::Unknown,
            ProjectDna::default(),
        );

        let mut plugin = CortexPlugin::new();
        plugin.on_init(&ctx).unwrap();

        wait_for_indexing_complete(&ctx, Duration::from_secs(10)).await;

        let metrics = ctx.metrics();
        assert_eq!(metrics.files_indexed, 0);
        assert_eq!(metrics.symbols_found, 0);
    }
}
