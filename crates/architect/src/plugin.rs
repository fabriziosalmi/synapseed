//! Architect Plugin — registers ReportStore and runs structural analysis.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tracing::{info, warn};

use synapseed_core::context::SynapseContext;
use synapseed_core::error::Result;
use synapseed_core::event::SynapseEvent;
use synapseed_core::plugin::SynapsePlugin;
use synapseed_cortex::graph::CodeGraph;

use crate::analyzer::DependencyGraph;
use crate::blueprint::{self, ReportStore};
use crate::linter::{self, LinterConfig};

/// The Architect plugin — structural analysis and health scoring.
pub struct ArchitectPlugin {
    store: Arc<ReportStore>,
}

impl ArchitectPlugin {
    pub fn new() -> Self {
        Self {
            store: Arc::new(ReportStore::new()),
        }
    }
}

impl Default for ArchitectPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl SynapsePlugin for ArchitectPlugin {
    fn name(&self) -> &str {
        "architect"
    }

    fn on_init(&mut self, ctx: &SynapseContext) -> Result<()> {
        ctx.set_extension(self.store.clone());

        if let Some(code_graph) = ctx.get_extension::<CodeGraph>() {
            let dna = ctx.dna();
            let linter_config = LinterConfig::from_dna(&dna.architect);
            let store = self.store.clone();

            let ctx_clone = ctx.clone();
            std::thread::spawn(move || {
                let mut dep_graph = DependencyGraph::build(&code_graph);
                dep_graph.compute_metrics();
                let violations = linter::lint(&dep_graph, &linter_config);
                let report = blueprint::generate_report(&dep_graph, violations);

                if ctx_clone.is_shutting_down() {
                    return;
                }

                info!(
                    score = report.score,
                    grade = %report.grade,
                    violations = report.violations.len(),
                    modules = report.module_count,
                    "Architect: Initial analysis complete"
                );

                store.set(report);
            });

            info!("Architect: Background analysis started");
        } else {
            warn!("Architect: CodeGraph not available — analysis deferred to first tool call");
        }

        Ok(())
    }

    fn on_event<'a>(
        &'a self,
        event: &'a SynapseEvent,
        ctx: &'a SynapseContext,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SynapseEvent>>> + Send + 'a>> {
        Box::pin(async move {
            if let SynapseEvent::FileChanged { .. } = event {
                if let Some(code_graph) = ctx.get_extension::<CodeGraph>() {
                    let dna = ctx.dna();
                    let linter_config = LinterConfig::from_dna(&dna.architect);
                    let store = self.store.clone();
                    let ctx_ev = ctx.clone();

                    std::thread::spawn(move || {
                        let mut dep_graph = DependencyGraph::build(&code_graph);
                        dep_graph.compute_metrics();
                        let violations = linter::lint(&dep_graph, &linter_config);
                        let report = blueprint::generate_report(&dep_graph, violations);
                        if !ctx_ev.is_shutting_down() {
                            store.set(report);
                        }
                    });
                }
            }
            Ok(None)
        })
    }

    fn priority(&self) -> u32 {
        150 // After cortex (50), before search (200)
    }
}
