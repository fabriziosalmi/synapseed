use serde_json::json;
use synapseed_core::context::SynapseContext;
use synapseed_cortex::graph::CodeGraph;

use super::{check_gitignore_warning, error_result, text_result};
use crate::protocol::ToolCallResult;

pub(super) fn tool_get_code_skeleton(
    args: &serde_json::Value,
    ctx: &SynapseContext,
) -> ToolCallResult {
    let root = ctx.project_root();
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .map(std::path::PathBuf::from)
        .unwrap_or(root.clone());

    // HCI Req 8: Honest Mirror — warn if path is gitignored
    let gi_warning = check_gitignore_warning(&path, &root).unwrap_or_default();

    // Try shared graph from CortexPlugin for project root
    if path == root {
        if let Some(graph) = ctx.get_extension::<CodeGraph>() {
            let summary = json!({
                "files_indexed": graph.file_count(),
                "symbols_indexed": graph.symbol_count(),
                "path": path.display().to_string(),
            });
            return text_result(format!(
                "{gi_warning}{}",
                serde_json::to_string_pretty(&summary).unwrap_or_default()
            ));
        }
    }

    // Fallback: build ephemeral graph
    let graph = CodeGraph::new();
    if let Err(e) = graph.index_directory(&path) {
        return error_result(format!("Failed to index: {e}"));
    }

    ctx.update_metrics(|m| {
        m.files_indexed = graph.file_count();
        m.symbols_found = graph.symbol_count();
    });

    let summary = json!({
        "files_indexed": graph.file_count(),
        "symbols_indexed": graph.symbol_count(),
        "path": path.display().to_string(),
    });

    text_result(serde_json::to_string_pretty(&summary).unwrap_or_default())
}
