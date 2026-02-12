//! MCP tool: oracle_fix_docs — auto-repair drifted documentation.

use synapseed_core::context::SynapseContext;

use super::text_result;
use crate::protocol::ToolCallResult;

/// Auto-fix drifted documentation (version, crate count, tool/resource counts).
pub(super) fn tool_oracle_fix_docs(ctx: &SynapseContext) -> ToolCallResult {
    let root = ctx.project_root();
    let changes = synapseed_core::oracle::fix_docs(&root);

    if changes.is_empty() {
        text_result("README.md is already up to date — no fixes needed.".into())
    } else {
        let summary = changes
            .iter()
            .map(|c| format!("  ✓ {c}"))
            .collect::<Vec<_>>()
            .join("\n");
        text_result(format!(
            "=== ORACLE FIX DOCS ===\n\
             Fixed {} inconsistencies in README.md:\n\n{summary}",
            changes.len()
        ))
    }
}
