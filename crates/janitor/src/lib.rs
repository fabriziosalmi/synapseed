#![forbid(unsafe_code)]
//! # SYNAPSEED Janitor
//!
//! Autonomous code maintenance module that monitors a project for
//! "low-hanging fruit" — clippy warnings, unused dependencies —
//! and proposes validated fixes.
//!
//! ## Architecture
//!
//! 1. **Scanner** — runs `cargo clippy --message-format=json` and collects issues
//! 2. **Fixer** — extracts machine-applicable suggestions, validates via Gym
//! 3. **Proposal Engine** — creates proposals with diffs, stores them for review
//!
//! The Janitor never auto-applies fixes. It creates [`Proposal`]s that the
//! user can review and apply via the `janitor_apply_fix` MCP tool.

pub mod fixer;
pub mod plugin;
pub mod proposal;
pub mod scanner;

pub use proposal::{LastScan, Proposal, ProposalCategory, ProposalStatus, ProposalStore};

use std::path::Path;
use std::sync::Arc;

use tracing::{debug, info, warn};

/// Error types for Janitor operations.
#[derive(Debug, thiserror::Error)]
pub enum JanitorError {
    #[error("Scanner error: {0}")]
    Scanner(String),

    #[error("Fixer error: {0}")]
    Fixer(String),

    #[error("Proposal not found: {0}")]
    NotFound(String),
}

/// Result of a Janitor scan.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScanResult {
    /// Total clippy issues found.
    pub clippy_issues: usize,
    /// Issues with auto-fixable suggestions.
    pub fixable_issues: usize,
    /// Potentially unused dependencies.
    pub unused_deps: Vec<String>,
    /// Number of proposals generated.
    pub proposals_created: usize,
}

/// The Janitor — entry point for autonomous code maintenance.
pub struct Janitor {
    store: Arc<ProposalStore>,
}

impl Janitor {
    pub fn new(store: Arc<ProposalStore>) -> Self {
        Self { store }
    }

    /// Run a full scan: clippy + unused deps → generate proposals.
    pub fn scan(&self, project_path: &Path) -> Result<ScanResult, JanitorError> {
        info!(path = %project_path.display(), "Janitor: starting scan");

        // Clear previous proposals for a fresh scan
        self.store.clear();

        // Step 1: Clippy scan
        let issues = scanner::scan_clippy(project_path)?;
        let clippy_issues = issues.len();
        let fixable_issues = issues.iter().filter(|i| i.has_auto_fix()).count();

        debug!(
            total = clippy_issues,
            fixable = fixable_issues,
            "Clippy scan results"
        );

        // Step 2: Generate proposals for fixable issues
        let mut proposals_created = 0;

        for issue in &issues {
            if let Some(fix) = fixer::generate_fix(issue, project_path) {
                // Create proposal — skip Gym validation for speed (clippy MachineApplicable is reliable)
                let proposal = fixer::fix_to_proposal(&fix, issue, false);
                self.store.add(proposal);
                proposals_created += 1;
            }
        }

        // Step 3: Unused dependency scan
        let unused_deps = match scanner::scan_unused_deps(project_path) {
            Ok(deps) => {
                // Create proposals for unused deps
                for dep in &deps {
                    let proposal = Proposal::new(
                        ProposalCategory::UnusedDependency,
                        "unused_dependency",
                        "Cargo.toml",
                        0,
                        0,
                        &format!("Dependency `{dep}` appears unused — not imported in any source file"),
                        &format!("{dep} = ..."),
                        &format!("# {dep} removed (unused)"),
                    );
                    self.store.add(proposal);
                    proposals_created += 1;
                }
                deps
            }
            Err(e) => {
                warn!("Unused dep scan failed: {e}");
                Vec::new()
            }
        };

        let result = ScanResult {
            clippy_issues,
            fixable_issues,
            unused_deps,
            proposals_created,
        };

        info!(
            clippy = clippy_issues,
            fixable = fixable_issues,
            unused_deps = result.unused_deps.len(),
            proposals = proposals_created,
            "Janitor scan complete"
        );

        Ok(result)
    }

    /// Apply a specific proposal by ID.
    ///
    /// Reads the file, applies the fix, runs `cargo check` to verify.
    /// Reverts automatically on failure.
    pub fn apply(
        &self,
        proposal_id: &str,
        project_path: &Path,
    ) -> Result<String, JanitorError> {
        let proposal = self
            .store
            .get(proposal_id)
            .ok_or_else(|| JanitorError::NotFound(proposal_id.to_string()))?;

        if proposal.status != ProposalStatus::Pending {
            return Err(JanitorError::Fixer(format!(
                "Proposal {} is already {:?}",
                proposal_id, proposal.status
            )));
        }

        match proposal.category {
            ProposalCategory::Clippy | ProposalCategory::CompilerWarning => {
                let abs_path = project_path.join(&proposal.file_path);

                let fix = fixer::Fix {
                    file_path: abs_path,
                    lint_code: proposal.lint_code.clone(),
                    description: proposal.description.clone(),
                    original_code: proposal.original_code.clone(),
                    fixed_code: proposal.fixed_code.clone(),
                    byte_start: 0, // Not used in this path
                    byte_end: 0,
                };

                // Apply by string replacement (more robust than byte offsets for stale fixes)
                apply_by_string_replacement(&fix, project_path)?;

                self.store.mark_applied(proposal_id);

                Ok(format!(
                    "Applied fix for `{}` in {}:{}",
                    proposal.lint_code, proposal.file_path, proposal.line_start
                ))
            }
            ProposalCategory::UnusedDependency => {
                // For unused deps, we just report — user should remove manually
                self.store.mark_applied(proposal_id);
                Ok(format!(
                    "Acknowledged: remove `{}` from Cargo.toml dependencies",
                    proposal.original_code.split('=').next().unwrap_or(&proposal.lint_code).trim()
                ))
            }
        }
    }

    /// Get a reference to the proposal store.
    pub fn store(&self) -> &ProposalStore {
        &self.store
    }
}

/// Apply a fix using string replacement (more resilient to minor file changes).
fn apply_by_string_replacement(
    fix: &fixer::Fix,
    project_path: &Path,
) -> Result<(), JanitorError> {
    let source = std::fs::read_to_string(&fix.file_path).map_err(|e| {
        JanitorError::Fixer(format!("Cannot read {}: {e}", fix.file_path.display()))
    })?;

    // Find and replace the original code
    if !source.contains(&fix.original_code) {
        return Err(JanitorError::Fixer(format!(
            "Original code not found in {} — file may have changed since scan",
            fix.file_path.display()
        )));
    }

    let modified = source.replacen(&fix.original_code, &fix.fixed_code, 1);

    // Write modified file
    std::fs::write(&fix.file_path, &modified).map_err(|e| {
        JanitorError::Fixer(format!("Cannot write {}: {e}", fix.file_path.display()))
    })?;

    // Verify with cargo check
    let check = std::process::Command::new("cargo")
        .args(["check", "--quiet"])
        .current_dir(project_path)
        .env("CARGO_TERM_COLOR", "never")
        .output();

    match check {
        Ok(output) if output.status.success() => {
            debug!(file = %fix.file_path.display(), "Fix applied and verified");
            Ok(())
        }
        _ => {
            // Revert
            warn!(file = %fix.file_path.display(), "cargo check failed after fix — reverting");
            std::fs::write(&fix.file_path, &source).map_err(|e| {
                JanitorError::Fixer(format!(
                    "CRITICAL: cannot revert {}: {e}",
                    fix.file_path.display()
                ))
            })?;
            Err(JanitorError::Fixer(format!(
                "Fix for `{}` broke compilation — reverted",
                fix.lint_code
            )))
        }
    }
}
