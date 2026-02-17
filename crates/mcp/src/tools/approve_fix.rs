//! `approve_fix` MCP tool — Human-in-the-loop gate for auto-repair proposals.
//!
//! The RepairOrchestrator creates proposals; this tool lets the client
//! (or the LLM) preview and approve them.  Matches the Janitor's
//! propose-then-confirm pattern for consistency.


use synapseed_core::context::SynapseContext;
use synapseed_core::event::SynapseEvent;
use synapseed_janitor::proposal::ProposalStore;
use synapseed_shadow_check::runner::DiagnosticStore;

use super::{error_result, text_result, ToolCallResult};
use crate::notification_sink::{Notification, NotificationSink};

/// Execute the `approve_fix` tool.
///
/// Params:
/// - `proposal_id` (required): UUID of the proposal to approve.
/// - `confirm` (optional, default false): if false, returns a preview (dry-run).
pub(super) fn tool_approve_fix(
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
        None => return error_result("ProposalStore not available.".into()),
    };

    // Retrieve the proposal
    let proposal = match store.get(proposal_id) {
        Some(p) => p,
        None => return error_result(format!("No proposal found with ID: {proposal_id}")),
    };

    // Dry-run: preview mode
    if !confirm {
        return text_result(format!(
            "PREVIEW (dry-run) — Auto-Repair Proposal {}\n\n\
             File: {}:{}-{}\n\
             Error: {} ({})\n\
             Description: {}\n\
             Replacement:\n```\n{}\n```\n\n\
             Call again with `confirm: true` to apply this fix.\n\
             The fix will be verified with `cargo check` and auto-reverted on failure.",
            proposal.id,
            proposal.file_path,
            proposal.line_start,
            proposal.line_end,
            proposal.lint_code,
            proposal.category_label(),
            proposal.description,
            proposal.fixed_code,
        ));
    }

    // Apply: use shadow-check's apply_fix mechanism
    let diag_store = match ctx.get_extension::<DiagnosticStore>() {
        Some(s) => s,
        None => return error_result("DiagnosticStore not available.".into()),
    };

    let _root = ctx.project_root();
    let file_path = &proposal.file_path;
    let error_code = &proposal.lint_code;

    match diag_store.apply_fix(file_path, error_code) {
        Ok(msg) => {
            store.mark_applied(proposal_id);

            // Notify client
            if let Some(sink) = ctx.get_extension::<NotificationSink>() {
                sink.send(Notification::auto_fix_applied(proposal_id, file_path, true));
            }

            // Broadcast success event
            ctx.broadcast(SynapseEvent::AutoFixApplied {
                proposal_id: proposal_id.to_string(),
                file_path: file_path.clone(),
                success: true,
            });

            text_result(format!(
                "Auto-fix applied successfully.\n\n{msg}\n\n\
                 Proposal {} marked as applied.",
                proposal_id,
            ))
        }
        Err(e) => {
            // Fix failed (reverted automatically by DiagnosticStore)
            if let Some(sink) = ctx.get_extension::<NotificationSink>() {
                sink.send(Notification::auto_fix_applied(proposal_id, file_path, false));
            }

            ctx.broadcast(SynapseEvent::AutoFixApplied {
                proposal_id: proposal_id.to_string(),
                file_path: file_path.clone(),
                success: false,
            });

            error_result(format!(
                "Auto-fix failed (reverted to original): {e}\n\
                 Proposal {} remains pending.",
                proposal_id,
            ))
        }
    }
}
