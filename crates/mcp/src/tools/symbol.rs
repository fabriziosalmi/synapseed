use synapseed_core::context::SynapseContext;
use synapseed_cortex::graph::CodeGraph;

use super::{error_result, text_result};
use crate::protocol::ToolCallResult;

pub(super) fn tool_lookup_symbol(
    args: &serde_json::Value,
    ctx: &SynapseContext,
) -> ToolCallResult {
    let name = match args.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return error_result("Missing required parameter: name".into()),
    };

    // Try shared graph from CortexPlugin
    if let Some(graph) = ctx.get_extension::<CodeGraph>() {
        let results = graph.lookup(name);
        return if results.is_empty() {
            text_result(format!("No symbols found matching '{name}'"))
        } else {
            let json = serde_json::to_string_pretty(&results).unwrap_or_default();
            text_result(format!("Found {} symbol(s):\n{json}", results.len()))
        };
    }

    // Fallback: build ephemeral graph
    let root = ctx.project_root();
    let graph = CodeGraph::new();
    let _ = graph.index_directory(&root);
    let results = graph.lookup(name);

    if results.is_empty() {
        text_result(format!("No symbols found matching '{name}'"))
    } else {
        let json = serde_json::to_string_pretty(&results).unwrap_or_default();
        text_result(format!("Found {} symbol(s):\n{json}", results.len()))
    }
}
