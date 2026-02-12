use synapseed_core::context::SynapseContext;
use synapseed_cortex::graph::CodeGraph;

use super::{CodeContext, Intent, Target};

pub(super) fn gather_code_context(
    intent: &Intent,
    targets: &[Target],
    ctx: &SynapseContext,
) -> Option<CodeContext> {
    if !matches!(
        intent,
        Intent::BugFix | Intent::Explain | Intent::Refactor | Intent::General
    ) {
        return None;
    }

    let root = ctx.project_root();
    let graph = CodeGraph::new();
    graph.index_directory(&root).ok()?;

    let mut symbols = Vec::new();
    for target in targets {
        for sym in graph.lookup(&target.name).into_iter().take(3) {
            symbols.push(serde_json::to_value(&sym).unwrap_or_default());
        }
    }

    if symbols.is_empty() {
        return None;
    }

    // Dedup by symbol name
    symbols.dedup_by(|a, b| a["name"] == b["name"]);
    Some(CodeContext { symbols })
}
