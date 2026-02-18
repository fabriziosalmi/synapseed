use std::path::Path;
use std::process::Command;

use synapseed_core::error::safe_resolve_path;
use tracing::{debug, info, warn};

use crate::proposal::{Proposal, ProposalCategory};
use crate::scanner::ClippyIssue;

/// A concrete fix ready to be applied to disk.
#[derive(Debug, Clone)]
pub struct Fix {
    /// Absolute path to the file.
    pub file_path: std::path::PathBuf,
    /// Lint code that triggered this fix.
    pub lint_code: String,
    /// Human-readable description.
    pub description: String,
    /// The original code that will be replaced.
    pub original_code: String,
    /// The fixed replacement code.
    pub fixed_code: String,
    /// Byte offset in the original file where replacement starts.
    pub byte_start: usize,
    /// Byte offset in the original file where replacement ends.
    pub byte_end: usize,
}

/// Generate a Fix from a ClippyIssue if it has a MachineApplicable suggestion.
///
/// Reads the source file, extracts the code at the suggestion's byte range,
/// and creates a Fix with the original and replacement snippets.
pub fn generate_fix(issue: &ClippyIssue, project_path: &Path) -> Option<Fix> {
    let suggestion = issue.auto_fix()?;

    let abs_path = safe_resolve_path(project_path, &suggestion.file_path).ok()?;
    let source = std::fs::read_to_string(&abs_path).ok()?;

    // Validate byte offsets are within the file
    if suggestion.byte_start > source.len() || suggestion.byte_end > source.len() {
        warn!(
            file = %suggestion.file_path,
            byte_start = suggestion.byte_start,
            byte_end = suggestion.byte_end,
            file_len = source.len(),
            "Suggestion byte range out of bounds"
        );
        return None;
    }

    let original_code = source[suggestion.byte_start..suggestion.byte_end].to_string();

    Some(Fix {
        file_path: abs_path,
        lint_code: issue.lint_code.clone(),
        description: issue.message.clone(),
        original_code,
        fixed_code: suggestion.replacement.clone(),
        byte_start: suggestion.byte_start,
        byte_end: suggestion.byte_end,
    })
}

/// Convert a Fix into a Proposal, optionally validating via the Gym.
pub fn fix_to_proposal(fix: &Fix, issue: &ClippyIssue, validate: bool) -> Proposal {
    let mut proposal = Proposal::new(
        ProposalCategory::Clippy,
        &fix.lint_code,
        &issue.file_path,
        issue.line_start,
        issue.line_end,
        &fix.description,
        &fix.original_code,
        &fix.fixed_code,
    );

    // MachineApplicable suggestions are validated by the compiler
    proposal.validated = true;

    // Optional: validate via the Gym (best-effort for self-contained code)
    if validate {
        proposal.gym_score = gym_validate(fix);
    }

    proposal
}

/// Validate a fix by running the modified file through the Gym.
///
/// Creates a Scenario with the entire file content (after applying the fix)
/// and checks if it compiles in isolation. Returns None if the code has
/// cross-module dependencies that prevent isolated compilation.
fn gym_validate(fix: &Fix) -> Option<f64> {
    let source = std::fs::read_to_string(&fix.file_path).ok()?;

    // Apply the fix to get the full modified file
    let mut modified = String::with_capacity(source.len());
    modified.push_str(&source[..fix.byte_start]);
    modified.push_str(&fix.fixed_code);
    modified.push_str(&source[fix.byte_end..]);

    let trainer = synapseed_gym::Trainer::new();
    let scenario = synapseed_gym::Scenario::new(&modified);

    match trainer.evaluate(&scenario) {
        Ok(report) if report.compilation.compiled => {
            debug!(
                lint = %fix.lint_code,
                score = report.score(),
                "Gym validation passed"
            );
            Some(report.score())
        }
        Ok(_) => {
            debug!(lint = %fix.lint_code, "Gym validation: code doesn't compile in isolation");
            None
        }
        Err(e) => {
            debug!(lint = %fix.lint_code, error = %e, "Gym validation failed");
            None
        }
    }
}

/// Apply a fix to the actual file on disk.
///
/// Safety: backs up the original content, applies the fix, then runs
/// `cargo check` to verify. Reverts on failure.
pub fn apply_fix(fix: &Fix, project_path: &Path) -> Result<(), crate::JanitorError> {
    let source = std::fs::read_to_string(&fix.file_path).map_err(|e| {
        crate::JanitorError::Fixer(format!("Cannot read {}: {e}", fix.file_path.display()))
    })?;

    // Verify the original code is still present (file hasn't changed since scan)
    let actual_snippet = &source[fix.byte_start..fix.byte_end.min(source.len())];
    if actual_snippet != fix.original_code {
        return Err(crate::JanitorError::Fixer(format!(
            "File {} has changed since scan — fix is stale",
            fix.file_path.display()
        )));
    }

    // Apply the fix
    let mut modified = String::with_capacity(source.len());
    modified.push_str(&source[..fix.byte_start]);
    modified.push_str(&fix.fixed_code);
    modified.push_str(&source[fix.byte_end..]);

    // Write the modified file
    std::fs::write(&fix.file_path, &modified).map_err(|e| {
        crate::JanitorError::Fixer(format!("Cannot write {}: {e}", fix.file_path.display()))
    })?;

    // Verify with cargo check
    let check = Command::new("cargo")
        .args(["check", "--quiet"])
        .current_dir(project_path)
        .env("CARGO_TERM_COLOR", "never")
        .output();

    match check {
        Ok(output) if output.status.success() => {
            debug!(
                file = %fix.file_path.display(),
                lint = %fix.lint_code,
                "Fix applied and verified (D30: suggest Co-authored-by trailer)"
            );
            // D30: Log provenance hint so commits can attribute AI-assisted changes.
            info!(
                "Janitor fix applied for {} — suggested commit trailer: Co-authored-by: Synapseed <synapseed@noreply>",
                fix.lint_code
            );
            Ok(())
        }
        _ => {
            // Revert: restore original content
            warn!(file = %fix.file_path.display(), "cargo check failed after fix — reverting");
            std::fs::write(&fix.file_path, &source).map_err(|e| {
                crate::JanitorError::Fixer(format!(
                    "CRITICAL: cannot revert {}: {e}",
                    fix.file_path.display()
                ))
            })?;
            Err(crate::JanitorError::Fixer(format!(
                "Fix for {} broke compilation — reverted",
                fix.lint_code
            )))
        }
    }
}
