use synapseed_core::context::SynapseContext;
use synapseed_core::state::ProjectState;

use super::{error_result, get_historian, text_result};
use crate::protocol::ToolCallResult;

pub(super) fn tool_project_diagnose(ctx: &SynapseContext) -> ToolCallResult {
    let root = ctx.project_root();
    let state = ProjectState::detect(&root);

    let mut report = format!("=== SYNAPSEED DIAGNOSTIC ===\n\n{}\n", state.diagnostic());

    // Git info
    if let Ok(historian) = get_historian(ctx) {
        if let Ok(summary) = historian.summary(5) {
            report.push_str(&format!(
                "\n--- Git ---\nBranch: {}\nHEAD: {}\nCommits: {}\nDirty: {}\n",
                summary.branch.as_deref().unwrap_or("detached"),
                &summary.head_commit[..8.min(summary.head_commit.len())],
                summary.total_commits,
                summary.is_dirty,
            ));
            if !summary.recent_commits.is_empty() {
                report.push_str("\nRecent:\n");
                for c in &summary.recent_commits {
                    report.push_str(&format!("  {} | {} | {}\n", c.id, c.author, c.message));
                }
            }
        }
    }

    // Metrics
    let metrics = ctx.metrics();
    report.push_str(&format!(
        "\n--- Metrics ---\nFiles: {} | Symbols: {} | DLP Blocks: {} | Events: {}\n",
        metrics.files_indexed, metrics.symbols_found, metrics.dlp_blocks, metrics.events_broadcast,
    ));

    text_result(report)
}

pub(super) fn tool_consult_architect(
    args: &serde_json::Value,
    ctx: &SynapseContext,
) -> ToolCallResult {
    let query = match args.get("query").and_then(|v| v.as_str()) {
        Some(q) => q,
        None => return error_result("Missing required parameter: query".into()),
    };

    let dna = ctx.dna();
    let state = ctx.project_state();

    let libs_list = dna
        .preferred_libs
        .iter()
        .map(|(k, v)| format!("  - {k}: {v}"))
        .collect::<Vec<_>>()
        .join("\n");

    let state_summary = match &state {
        ProjectState::HealthyWorkspace {
            build_system,
            file_count,
        } => {
            format!("Healthy ({build_system:?}, {file_count} files)")
        }
        ProjectState::VirginRepo => "Virgin repository (no code yet)".into(),
        ProjectState::PartialSetup { missing, .. } => {
            format!("Partial setup (missing: {})", missing.join(", "))
        }
        ProjectState::Unknown => "Unknown project type".into(),
    };

    let policy = format!(
        "=== ARCHITECTURE POLICY ===\n\n\
         Query: {query}\n\n\
         --- Project DNA ---\n\
         Workspace Strategy: {}\n\
         Naming: core_crate={}, bin_name={}\n\
         DLP Level: {:?}\n\
         Active Plugins: {}\n\n\
         --- Preferred Libraries ---\n\
         {libs_list}\n\n\
         --- Project State ---\n\
         {state_summary}\n\n\
         --- Architecture Guidance ---\n\
         1. Use {} workspace strategy\n\
         2. Async runtime: {}\n\
         3. Error handling: {}\n\
         4. Serialization: {}\n\
         5. Security: DLP level {:?} with fail-closed sentinel\n\
         6. All commands MUST pass through the Sentinel before execution\n\
         7. All outbound content MUST pass through DLP scanning\n",
        dna.workspace_strategy,
        dna.naming.core_crate,
        dna.naming.bin_name,
        dna.dlp_level,
        dna.plugins.join(", "),
        dna.workspace_strategy,
        dna.preferred_libs
            .get("async")
            .map(|s| s.as_str())
            .unwrap_or("tokio"),
        dna.preferred_libs
            .get("error")
            .map(|s| s.as_str())
            .unwrap_or("thiserror"),
        dna.preferred_libs
            .get("json")
            .map(|s| s.as_str())
            .unwrap_or("serde_json"),
        dna.dlp_level,
    );

    text_result(policy)
}
