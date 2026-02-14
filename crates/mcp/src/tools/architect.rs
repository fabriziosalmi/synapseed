use parking_lot::Mutex;
use synapseed_architect::ReportStore;
use synapseed_core::context::SynapseContext;
use synapseed_core::recorder::FlightRecorder;
use synapseed_cortex::graph::CodeGraph;

use super::{error_result, text_result};
use crate::protocol::ToolCallResult;

pub(super) fn tool_architect_analyze(
    args: &serde_json::Value,
    ctx: &SynapseContext,
) -> ToolCallResult {
    let refresh = args
        .get("refresh")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Try cached report from ArchitectPlugin
    if !refresh {
        if let Some(store) = ctx.get_extension::<ReportStore>() {
            if let Some(report) = store.get() {
                let json = serde_json::to_string_pretty(&report).unwrap_or_default();
                return text_result(format!(
                    "=== ARCHITECTURE REPORT ===\nScore: {}/100 (Grade: {})\nModules: {} | Edges: {} | Violations: {}\n\n{json}",
                    report.score, report.grade, report.module_count, report.edge_count, report.violations.len()
                ));
            }
        }
    }

    // Build fresh report (or no cached one exists)
    let graph = match ctx.get_extension::<CodeGraph>() {
        Some(g) => g,
        None => {
            // Fallback: build ephemeral graph
            let root = ctx.project_root();
            let g = CodeGraph::new();
            if let Err(e) = g.index_directory(&root) {
                return error_result(format!("Failed to index project: {e}"));
            }
            std::sync::Arc::new(g)
        }
    };

    let dna = ctx.dna();
    let mut dep_graph = synapseed_architect::DependencyGraph::build(&graph);
    dep_graph.compute_metrics();

    let config = synapseed_architect::linter::LinterConfig::from_dna(&dna.architect);
    let violations = synapseed_architect::linter::lint(&dep_graph, &config);
    let report = synapseed_architect::blueprint::generate_report(&dep_graph, violations);

    // Feed dependency hints to the Flight Recorder for causal link detection
    if let Some(recorder) = ctx.get_extension::<Mutex<FlightRecorder>>() {
        recorder.lock().set_dep_hints(dep_graph.dep_pairs());
    }

    // Cache the report if store exists
    if let Some(store) = ctx.get_extension::<ReportStore>() {
        store.set(report.clone());
    }

    let json = serde_json::to_string_pretty(&report).unwrap_or_default();
    text_result(format!(
        "=== ARCHITECTURE REPORT ===\nScore: {}/100 (Grade: {})\nModules: {} | Edges: {} | Violations: {}\n\n{json}",
        report.score, report.grade, report.module_count, report.edge_count, report.violations.len()
    ))
}
