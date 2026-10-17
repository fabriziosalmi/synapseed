use synapseed_core::context::SynapseContext;
use synapseed_core::symbol::Symbol;
use synapseed_cortex::graph::CodeGraph;

use super::{error_result, text_result};
use crate::protocol::ToolCallResult;

/// D42: Maximum symbols returned by `lookup` to avoid blowing the context window.
const LOOKUP_RESULT_CAP: usize = 20;

pub(super) fn tool_lookup_symbol(args: &serde_json::Value, ctx: &SynapseContext) -> ToolCallResult {
    let name = match args.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return error_result("Missing required parameter: name".into()),
    };

    // Try shared graph from CortexPlugin
    // D45: detect cold-start (graph registered but not yet populated)
    let cold_start = ctx
        .get_extension::<CodeGraph>()
        .is_none_or(|g| g.file_count() == 0);

    if let Some(graph) = ctx.get_extension::<CodeGraph>() {
        let results = graph.lookup(name);
        return format_lookup_results(name, results, cold_start);
    }

    // Fallback: build ephemeral graph
    let root = ctx.project_root();
    let graph = CodeGraph::new();
    let _ = graph.index_directory(&root);
    let results = graph.lookup(name);

    format_lookup_results(name, results, false)
}

/// D42: Format lookup results with a cap and statistical summary for large result sets.
/// D45: Prepend cold-start warning if indexing is still in progress.
fn format_lookup_results(name: &str, results: Vec<Symbol>, cold_start: bool) -> ToolCallResult {
    let prefix = if cold_start {
        "⚠ Indexing may still be in progress — results could be incomplete. Retry shortly for full coverage.\n\n"
    } else {
        ""
    };

    if results.is_empty() {
        return text_result(format!("{prefix}No symbols found matching '{name}'"));
    }

    let total = results.len();
    if total <= LOOKUP_RESULT_CAP {
        let json = serde_json::to_string_pretty(&results).unwrap_or_default();
        return text_result(format!("{prefix}Found {total} symbol(s):\n{json}"));
    }

    // D42: Statistical summary for large result sets instead of blowing the context window.
    // Group by kind and file to give the model actionable orientation.
    let mut by_kind: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut by_file: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for sym in &results {
        *by_kind.entry(format!("{:?}", sym.kind)).or_default() += 1;
        *by_file.entry(sym.file_path.clone()).or_default() += 1;
    }

    let mut kind_summary: Vec<_> = by_kind.into_iter().collect();
    kind_summary.sort_by_key(|k| std::cmp::Reverse(k.1));
    let kind_str: Vec<String> = kind_summary
        .iter()
        .map(|(k, n)| format!("{k}: {n}"))
        .collect();

    let mut file_summary: Vec<_> = by_file.into_iter().collect();
    file_summary.sort_by_key(|f| std::cmp::Reverse(f.1));
    let top_files: Vec<String> = file_summary
        .iter()
        .take(5)
        .map(|(f, n)| format!("  {f} ({n})"))
        .collect();

    let shown: Vec<_> = results.into_iter().take(LOOKUP_RESULT_CAP).collect();
    let json = serde_json::to_string_pretty(&shown).unwrap_or_default();

    text_result(format!(
        "{prefix}Found {total} symbol(s) matching '{name}' (showing first {LOOKUP_RESULT_CAP}):\n\n\
         Distribution by kind: {kind_list}\n\
         Top files:\n{top_files}\n\n\
         To narrow results, search for a more specific name.\n\n{json}",
        kind_list = kind_str.join(", "),
        top_files = top_files.join("\n"),
    ))
}
