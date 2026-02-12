use synapseed_core::context::SynapseContext;
use synapseed_janitor::{Janitor, ProposalStore};

use super::{error_result, text_result};
use crate::protocol::ToolCallResult;

pub(super) fn tool_janitor_run_now(ctx: &SynapseContext) -> ToolCallResult {
    let store = match ctx.get_extension::<ProposalStore>() {
        Some(s) => s,
        None => return error_result("Janitor plugin not active.".into()),
    };

    // Prevent double-scan (atomic compare-exchange)
    if !store.start_scanning() {
        return text_result(
            "Janitor scan already in progress. Check `synapseed://janitor/proposals` for results."
                .into(),
        );
    }

    // If there's a previous scan result, include it as context
    let previous = store.last_scan().map(|s| {
        format!(
            " (previous scan: {} issues, {} proposals at {})",
            s.clippy_issues, s.proposals_created, s.completed_at
        )
    });

    let root = ctx.project_root().to_path_buf();
    let bg_store = store.clone();

    // Run scan in background thread — return immediately
    std::thread::spawn(move || {
        let janitor = Janitor::new(bg_store.clone());
        match janitor.scan(&root) {
            Ok(result) => {
                bg_store.finish_scan(synapseed_janitor::LastScan {
                    completed_at: chrono::Utc::now().to_rfc3339(),
                    clippy_issues: result.clippy_issues,
                    fixable_issues: result.fixable_issues,
                    unused_deps: result.unused_deps.len(),
                    proposals_created: result.proposals_created,
                    error: None,
                });
            }
            Err(e) => {
                bg_store.finish_scan(synapseed_janitor::LastScan {
                    completed_at: chrono::Utc::now().to_rfc3339(),
                    clippy_issues: 0,
                    fixable_issues: 0,
                    unused_deps: 0,
                    proposals_created: 0,
                    error: Some(e.to_string()),
                });
            }
        }
    });

    text_result(format!(
        "Janitor scan started in background.{}\n\nResults will appear in `synapseed://janitor/proposals`. You can also call `janitor_run_now` again — it will show the results once the scan completes.",
        previous.unwrap_or_default()
    ))
}

pub(super) fn tool_janitor_apply_fix(
    args: &serde_json::Value,
    ctx: &SynapseContext,
) -> ToolCallResult {
    let proposal_id = match args.get("proposal_id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return error_result("Missing required parameter: proposal_id".into()),
    };
    let confirm = args
        .get("confirm")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let store = match ctx.get_extension::<ProposalStore>() {
        Some(s) => s,
        None => return error_result("Janitor plugin not active.".into()),
    };

    // HCI Req 3 (Safety Net): dry-run by default — preview what WOULD change
    if !confirm {
        return match store.get(proposal_id) {
            Some(proposal) => text_result(format!(
                "PREVIEW (dry-run): Would apply fix to {}:{}\n\
                 - Description: {}\n\
                 - Original:\n{}\n\
                 - Fixed:\n{}\n\n\
                 Call again with `confirm: true` to apply this fix.",
                proposal.file_path,
                proposal.line_start,
                proposal.description,
                proposal.original_code,
                proposal.fixed_code,
            )),
            None => error_result(format!("No proposal found with ID: {proposal_id}")),
        };
    }

    let janitor = Janitor::new(store);
    let root = ctx.project_root();

    match janitor.apply(proposal_id, &root) {
        Ok(msg) => text_result(format!("Fix applied successfully.\n{msg}")),
        Err(e) => error_result(format!("Failed to apply fix: {e}")),
    }
}
