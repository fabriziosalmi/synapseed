//! RepairOrchestrator — Semi-autonomous self-healing via compiler suggestions.
//!
//! Listens for `DiagnosticUpdated` events on the bus.  When new diagnostics
//! contain `MachineApplicable` suggestions, creates proposals in the shared
//! `ProposalStore` and notifies the client via `NotificationSink`.
//!
//! **Human-in-the-loop**: fixes are PROPOSED, not applied.  The client (or LLM)
//! must call `approve_fix` to actually apply.  This is the "Semi-Autonomia
//! Assistita" pattern — deterministic fix generation with human approval gate.
//!
//! Safety guardrails:
//! - Only `MachineApplicable` suggestions (safe by rustc standards)
//! - Max 5 proposals per session (prevent runaway)
//! - Cooldown: 5s between proposals for the same file
//! - Staleness check on apply (file must not have changed)

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use tracing::{debug, info};

use synapseed_core::context::SynapseContext;
use synapseed_core::event::SynapseEvent;
use synapseed_janitor::proposal::{Proposal, ProposalCategory, ProposalStore};
use synapseed_shadow_check::diagnostic::Applicability;
use synapseed_shadow_check::runner::DiagnosticStore;

use crate::notification_sink::{Notification, NotificationSink};

/// Maximum proposals the orchestrator will generate per session.
const MAX_PROPOSALS_PER_SESSION: u32 = 5;

/// Minimum seconds between proposals for the same file.
const FILE_COOLDOWN_SECS: u64 = 5;

/// Shared state for the repair orchestrator.
struct OrchestratorState {
    /// Tracks last proposal time per file for cooldown.
    file_cooldowns: HashMap<String, Instant>,
}

/// Session-wide proposal counter (atomic for cross-task visibility).
static PROPOSAL_COUNT: AtomicU32 = AtomicU32::new(0);

/// Spawn the RepairOrchestrator background task.
///
/// This task subscribes to the event bus, watches for `DiagnosticUpdated`,
/// and creates proposals for any `MachineApplicable` suggestions found.
pub fn spawn_repair_orchestrator(ctx: &SynapseContext) {
    let mut rx = ctx.subscribe();
    let ctx = ctx.clone();

    tokio::spawn(async move {
        let mut state = OrchestratorState {
            file_cooldowns: HashMap::new(),
        };

        loop {
            match rx.recv().await {
                Ok(SynapseEvent::DiagnosticUpdated { errors, warnings }) => {
                    if errors == 0 && warnings == 0 {
                        continue; // Clean build — nothing to repair
                    }
                    debug!(
                        errors,
                        warnings,
                        "RepairOrchestrator: diagnostics updated, scanning for auto-fixable issues"
                    );
                    process_diagnostics(&ctx, &mut state);
                }
                Ok(SynapseEvent::SystemShutdown) => {
                    info!("RepairOrchestrator: shutting down");
                    break;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    debug!(skipped = n, "RepairOrchestrator: lagged, dropped events");
                }
                Err(_) => break, // Channel closed
                _ => {}          // Ignore other events
            }
        }
    });
}

/// Scan current diagnostics for MachineApplicable suggestions and create proposals.
fn process_diagnostics(ctx: &SynapseContext, state: &mut OrchestratorState) {
    let diag_store = match ctx.get_extension::<DiagnosticStore>() {
        Some(s) => s,
        None => return,
    };
    let proposal_store = match ctx.get_extension::<ProposalStore>() {
        Some(s) => s,
        None => return,
    };

    // Guard: session proposal limit
    if PROPOSAL_COUNT.load(Ordering::Relaxed) >= MAX_PROPOSALS_PER_SESSION {
        debug!(
            "RepairOrchestrator: session proposal limit reached ({})",
            MAX_PROPOSALS_PER_SESSION
        );
        return;
    }

    let snapshot = diag_store.snapshot();
    let now = Instant::now();

    for diag in &snapshot.diagnostics {
        // Only process errors/warnings with MachineApplicable suggestions
        let suggestion = match diag
            .suggestions
            .iter()
            .find(|s| s.applicability == Applicability::MachineApplicable)
        {
            Some(s) => s,
            None => continue,
        };

        let error_code = match &diag.code {
            Some(c) => c.clone(),
            None => continue, // Skip diagnostics without error codes
        };

        // Cooldown: skip if we recently proposed for this file
        if let Some(last) = state.file_cooldowns.get(&diag.file_path) {
            if now.duration_since(*last).as_secs() < FILE_COOLDOWN_SECS {
                debug!(
                    file = %diag.file_path,
                    "RepairOrchestrator: cooldown active, skipping"
                );
                continue;
            }
        }

        // Guard: session proposal limit (re-check inside loop)
        if PROPOSAL_COUNT.load(Ordering::Relaxed) >= MAX_PROPOSALS_PER_SESSION {
            break;
        }

        // Check if we already have a pending proposal for this exact location
        let existing = proposal_store.pending().iter().any(|p| {
            p.file_path == diag.file_path
                && p.lint_code == error_code
                && p.line_start == diag.line_start as u32
        });
        if existing {
            continue;
        }

        // Create the proposal
        let proposal = Proposal::new(
            ProposalCategory::CompilerError,
            &error_code,
            &diag.file_path,
            diag.line_start as u32,
            diag.line_end as u32,
            &diag.message,
            "", // original_code not available from diagnostic alone
            &suggestion.replacement,
        );
        let proposal_id = proposal.id.clone();

        // Generate preview diff
        let preview = format!(
            "{}:{}-{}: {} → replace with: {}",
            diag.file_path,
            diag.line_start,
            diag.line_end,
            diag.message,
            truncate(&suggestion.replacement, 200),
        );

        proposal_store.add(proposal);
        state.file_cooldowns.insert(diag.file_path.clone(), now);
        PROPOSAL_COUNT.fetch_add(1, Ordering::Relaxed);

        info!(
            proposal_id = %proposal_id,
            file = %diag.file_path,
            error = %error_code,
            "RepairOrchestrator: auto-fix proposed"
        );

        // Notify client via NotificationSink
        if let Some(sink) = ctx.get_extension::<NotificationSink>() {
            sink.send(Notification::auto_fix_proposed(
                &proposal_id,
                &diag.file_path,
                &error_code,
                &preview,
            ));
        }

        // Broadcast the event for FlightRecorder and other consumers
        ctx.broadcast(SynapseEvent::AutoFixProposed {
            proposal_id,
            file_path: diag.file_path.clone(),
            error_code,
            preview,
        });
    }
}

/// Truncate a string to `max` chars with ellipsis.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

/// Reset the session proposal counter (for testing).
#[cfg(test)]
pub fn reset_proposal_count() {
    PROPOSAL_COUNT.store(0, Ordering::Relaxed);
}
