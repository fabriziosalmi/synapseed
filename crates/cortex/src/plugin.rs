use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use synapseed_core::context::SynapseContext;
use synapseed_core::error::Result;
use synapseed_core::event::{FileChangeKind, SynapseEvent};
use synapseed_core::plugin::SynapsePlugin;
use tracing::info;

use crate::graph::CodeGraph;
use crate::parser::AstParser;

/// The Cortex plugin — semantic code understanding.
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

        self.graph.index_directory(&root)?;

        ctx.update_metrics(|m| {
            m.files_indexed = self.graph.file_count();
            m.symbols_found = self.graph.symbol_count();
        });

        ctx.set_extension(self.graph.clone());

        info!(
            files = self.graph.file_count(),
            symbols = self.graph.symbol_count(),
            "Cortex: Code graph initialized"
        );

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
