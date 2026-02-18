//! Architect Plugin — registers ReportStore and runs structural analysis.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tracing::{info, warn};

use synapseed_core::context::SynapseContext;
use synapseed_core::error::Result;
use synapseed_core::event::SynapseEvent;
use synapseed_core::plugin::SynapsePlugin;
use synapseed_core::recorder::FlightRecorder;
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

                // D58: Broadcast architecture violations as domain events.
                for v in &violations {
                    ctx_clone.broadcast(SynapseEvent::ArchitectureViolation {
                        rule: v.rule.clone(),
                        severity: format!("{:?}", v.severity),
                        modules: v.modules.clone(),
                    });
                }

                let report = blueprint::generate_report(&dep_graph, violations);

                if ctx_clone.is_shutting_down() {
                    return;
                }

                // Feed dependency hints to FlightRecorder for causal tracking (#76)
                if let Some(rec) = ctx_clone.get_extension::<parking_lot::Mutex<FlightRecorder>>() {
                    let pairs = dep_graph.dep_pairs();
                    let count = pairs.len();
                    rec.lock().set_dep_hints(pairs);
                    info!(
                        dep_hints = count,
                        "Architect: Populated FlightRecorder dep_hints"
                    );
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
            let should_reanalyze = matches!(
                event,
                SynapseEvent::FileChanged { .. } | SynapseEvent::IndexingComplete
            );

            if should_reanalyze {
                if let Some(code_graph) = ctx.get_extension::<CodeGraph>() {
                    // Skip re-analysis if graph is still empty (indexing just started).
                    if code_graph.file_count() == 0 {
                        return Ok(None);
                    }

                    let dna = ctx.dna();
                    let linter_config = LinterConfig::from_dna(&dna.architect);
                    let store = self.store.clone();
                    let ctx_ev = ctx.clone();

                    std::thread::spawn(move || {
                        let mut dep_graph = DependencyGraph::build(&code_graph);
                        dep_graph.compute_metrics();
                        let violations = linter::lint(&dep_graph, &linter_config);

                        // D58: Broadcast architecture violations on re-analysis.
                        for v in &violations {
                            ctx_ev.broadcast(SynapseEvent::ArchitectureViolation {
                                rule: v.rule.clone(),
                                severity: format!("{:?}", v.severity),
                                modules: v.modules.clone(),
                            });
                        }

                        let report = blueprint::generate_report(&dep_graph, violations);
                        if !ctx_ev.is_shutting_down() {
                            // Refresh dep_hints in FlightRecorder (#76)
                            if let Some(rec) =
                                ctx_ev.get_extension::<parking_lot::Mutex<FlightRecorder>>()
                            {
                                rec.lock().set_dep_hints(dep_graph.dep_pairs());
                            }

                            info!(
                                modules = report.module_count,
                                score = report.score,
                                "Architect: Re-analysis complete"
                            );
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
