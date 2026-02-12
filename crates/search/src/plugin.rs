//! Search Plugin — builds and maintains the Tantivy semantic index.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tracing::{debug, info, warn};

use synapseed_core::context::SynapseContext;
use synapseed_core::error::Result;
use synapseed_core::event::{FileChangeKind, SynapseEvent};
use synapseed_core::plugin::SynapsePlugin;
use synapseed_cortex::graph::CodeGraph;
use synapseed_cortex::parser::AstParser;

use crate::indexer::SemanticIndex;

/// The Search plugin — semantic code search powered by Tantivy.
pub struct SearchPlugin {
    index: Option<Arc<SemanticIndex>>,
}

impl SearchPlugin {
    pub fn new() -> Self {
        Self { index: None }
    }
}

impl Default for SearchPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl SynapsePlugin for SearchPlugin {
    fn name(&self) -> &str {
        "search"
    }

    fn on_init(&mut self, ctx: &SynapseContext) -> Result<()> {
        let root = ctx.project_root();
        let dna = ctx.dna();

        // Create the semantic index (disk or RAM based on DNA config)
        let index = if dna.search.persistence {
            let index_dir = root.join(".synapseed").join("index");
            match SemanticIndex::open_or_create(&index_dir) {
                Ok(idx) => {
                    info!(path = %index_dir.display(), "Search: Using persistent disk index");
                    Arc::new(idx)
                }
                Err(e) => {
                    warn!(error = %e, "Search: Disk index failed, falling back to RAM");
                    match SemanticIndex::new() {
                        Ok(idx) => Arc::new(idx),
                        Err(e2) => {
                            warn!(error = %e2, "Search: Failed to create any index");
                            return Ok(());
                        }
                    }
                }
            }
        } else {
            match SemanticIndex::new() {
                Ok(idx) => Arc::new(idx),
                Err(e) => {
                    warn!(error = %e, "Search: Failed to create Tantivy index");
                    return Ok(()); // Non-fatal — search is optional
                }
            }
        };

        // Register the index immediately so MCP tool can use it
        // (searches before indexing completes return empty results;
        //  the tool has an ephemeral fallback anyway)
        ctx.set_extension(index.clone());
        self.index = Some(index.clone());

        // Bulk indexing runs in a background thread to avoid blocking MCP startup
        let bg_root = root.clone();
        std::thread::spawn(move || {
            let graph = CodeGraph::new();
            if let Err(e) = graph.index_directory(&bg_root) {
                warn!(error = %e, "Search: Failed to index project for search");
                return;
            }

            let files = graph.all_files();
            let count = index.index_all(&files, &bg_root);
            info!(symbols = count, "Search: Semantic index ready");
        });

        info!("Search: Background indexing started");
        Ok(())
    }

    fn on_event<'a>(
        &'a self,
        event: &'a SynapseEvent,
        ctx: &'a SynapseContext,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SynapseEvent>>> + Send + 'a>> {
        Box::pin(async move {
            if let SynapseEvent::FileChanged { path, kind } = event {
                if let Some(index) = &self.index {
                    let root = ctx.project_root();

                    match kind {
                        FileChangeKind::Deleted => {
                            index.remove_file(path);
                            debug!(file = path, "Search: Removed deleted file from index");
                        }
                        FileChangeKind::Created | FileChangeKind::Modified => {
                            // Re-parse and reindex the changed file
                            let file_path = std::path::Path::new(path);
                            if let Ok(mut parser) = AstParser::new() {
                                if let Ok(source) = std::fs::read_to_string(file_path) {
                                    if let Ok(file_structure) =
                                        parser.parse_file(file_path, &source)
                                    {
                                        let count = index.reindex_file(&file_structure, &root);
                                        debug!(
                                            file = path,
                                            symbols = count,
                                            "Search: Incremental reindex"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Ok(None)
        })
    }

    fn priority(&self) -> u32 {
        200 // After cortex (100) but before visualizer (250)
    }
}
