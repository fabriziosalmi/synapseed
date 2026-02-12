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
                    kind: FileChangeKind::Modified | FileChangeKind::Created,
                } => {
                    let file_path = std::path::Path::new(path);
                    if std::fs::read_to_string(file_path).is_ok() {
                        let symbols = self
                            .graph
                            .lookup(file_path.file_stem().and_then(|s| s.to_str()).unwrap_or(""));
                        if !symbols.is_empty() {
                            info!(
                                file = %path,
                                symbols = symbols.len(),
                                "Cortex: File change detected, symbols available"
                            );
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
