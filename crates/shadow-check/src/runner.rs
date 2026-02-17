//! Background runner — spawns `cargo check` and manages the diagnostic store.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;

use tracing::{debug, info, warn};

use synapseed_core::context::SynapseContext;
use synapseed_core::error::safe_resolve_path;
use synapseed_core::event::SynapseEvent;

use crate::diagnostic::{
    parse_cargo_line, Applicability, Diagnostic, DiagnosticLevel, DiagnosticSnapshot, Suggestion,
};

/// D33: Maximum age (in seconds) for shadow target directories.
/// Directories older than this are cleaned on startup.
const SHADOW_MAX_AGE_SECS: u64 = 7 * 24 * 3600; // 7 days

/// D33: Maximum total size (in bytes) for a single shadow target directory.
/// If exceeded, the directory is wiped and rebuilt from scratch.
const SHADOW_MAX_SIZE_BYTES: u64 = 2 * 1024 * 1024 * 1024; // 2 GB

/// Minimum severity filter for diagnostic queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MinSeverity {
    Info,
    Warning,
    Error,
}

impl MinSeverity {
    pub fn from_str_loose(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "error" | "err" => Self::Error,
            "warning" | "warn" => Self::Warning,
            _ => Self::Info,
        }
    }

    fn matches(&self, level: &DiagnosticLevel) -> bool {
        match self {
            Self::Info => true,
            Self::Warning => matches!(level, DiagnosticLevel::Warning | DiagnosticLevel::Error),
            Self::Error => matches!(level, DiagnosticLevel::Error),
        }
    }
}

/// The shared diagnostic store — thread-safe, accessible via `ctx.get_extension()`.
pub struct DiagnosticStore {
    inner: RwLock<StoreInner>,
}

struct StoreInner {
    diagnostics: Vec<Diagnostic>,
    error_count: usize,
    warning_count: usize,
    /// Counts from the previous check run (for trend detection — D25).
    prev_error_count: usize,
    prev_warning_count: usize,
    last_check_ms: u64,
    project_root: PathBuf,
}

impl DiagnosticStore {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            inner: RwLock::new(StoreInner {
                diagnostics: Vec::new(),
                error_count: 0,
                warning_count: 0,
                prev_error_count: 0,
                prev_warning_count: 0,
                last_check_ms: 0,
                project_root,
            }),
        }
    }

    /// Get a snapshot of current diagnostics.
    pub fn snapshot(&self) -> DiagnosticSnapshot {
        let inner = self.inner.read();
        DiagnosticSnapshot {
            diagnostics: inner.diagnostics.clone(),
            error_count: inner.error_count,
            warning_count: inner.warning_count,
            prev_error_count: inner.prev_error_count,
            prev_warning_count: inner.prev_warning_count,
            last_check_ms: inner.last_check_ms,
        }
    }

    /// Get a snapshot filtered by minimum severity.
    pub fn filtered_snapshot(&self, min: MinSeverity) -> DiagnosticSnapshot {
        let inner = self.inner.read();
        let filtered: Vec<Diagnostic> = inner
            .diagnostics
            .iter()
            .filter(|d| min.matches(&d.level))
            .cloned()
            .collect();
        let error_count = filtered
            .iter()
            .filter(|d| d.level == DiagnosticLevel::Error)
            .count();
        let warning_count = filtered
            .iter()
            .filter(|d| d.level == DiagnosticLevel::Warning)
            .count();
        DiagnosticSnapshot {
            diagnostics: filtered,
            error_count,
            warning_count,
            prev_error_count: inner.prev_error_count,
            prev_warning_count: inner.prev_warning_count,
            last_check_ms: inner.last_check_ms,
        }
    }

    /// Get diagnostics for a specific file.
    pub fn for_file(&self, file_path: &str) -> Vec<Diagnostic> {
        let inner = self.inner.read();
        inner
            .diagnostics
            .iter()
            .filter(|d| d.file_path == file_path || file_path.ends_with(&d.file_path))
            .cloned()
            .collect()
    }

    /// Find a specific suggestion by file path and error code.
    pub fn find_suggestion(
        &self,
        file_path: &str,
        error_code: &str,
    ) -> Option<(Diagnostic, Suggestion)> {
        let inner = self.inner.read();
        for diag in &inner.diagnostics {
            if (diag.file_path == file_path || file_path.ends_with(&diag.file_path))
                && diag.code.as_deref() == Some(error_code)
            {
                if let Some(suggestion) = diag
                    .suggestions
                    .iter()
                    .find(|s| s.applicability == Applicability::MachineApplicable)
                {
                    return Some((diag.clone(), suggestion.clone()));
                }
            }
        }
        None
    }

    /// Run cargo check and update the store. Returns (errors, warnings).
    pub fn run_check(&self) -> (usize, usize) {
        let project_root = {
            let inner = self.inner.read();
            inner.project_root.clone()
        };

        // Use a separate target directory to avoid lock contention with
        // the user's own `cargo build` / `cargo check` (Q12 fix).
        let shadow_target = std::env::temp_dir().join(format!(
            "synapseed-shadow-{:x}",
            fxhash::hash64(project_root.as_os_str().as_encoded_bytes())
        ));

        // D33: Enforce disk limits before running cargo check.
        cleanup_shadow_target(&shadow_target);

        let start = Instant::now();

        let output = Command::new("cargo")
            .args(["check", "--message-format=json", "--quiet"])
            .arg("--target-dir")
            .arg(&shadow_target)
            .current_dir(&project_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();

        let output = match output {
            Ok(o) => o,
            Err(e) => {
                warn!(error = %e, "Shadow: Failed to run cargo check");
                return (0, 0);
            }
        };

        let elapsed_ms = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout);

        let mut all_diags = Vec::new();
        for line in stdout.lines() {
            let mut diags = parse_cargo_line(line);
            all_diags.append(&mut diags);
        }

        // v4.17.1 (W5): Filter out diagnostics for files that no longer exist.
        // Cargo's incremental cache can emit stale diagnostics for deleted files.
        all_diags.retain(|d| project_root.join(&d.file_path).exists());

        let errors = all_diags
            .iter()
            .filter(|d| d.level == DiagnosticLevel::Error)
            .count();
        let warnings = all_diags
            .iter()
            .filter(|d| d.level == DiagnosticLevel::Warning)
            .count();

        // Update the store (D25: preserve previous counts for trend detection)
        {
            let mut inner = self.inner.write();
            inner.prev_error_count = inner.error_count;
            inner.prev_warning_count = inner.warning_count;
            inner.diagnostics = all_diags;
            inner.error_count = errors;
            inner.warning_count = warnings;
            inner.last_check_ms = elapsed_ms;
        }

        debug!(
            errors = errors,
            warnings = warnings,
            elapsed_ms = elapsed_ms,
            "Shadow: cargo check complete"
        );

        (errors, warnings)
    }

    /// Apply a quick fix: read the file, apply the replacement, write back.
    ///
    /// D35: After writing, runs `cargo check --quiet` to verify the fix.
    /// If compilation breaks, the original file content is restored.
    pub fn apply_fix(&self, file_path: &str, error_code: &str) -> Result<String, String> {
        let (_diag, suggestion) = self.find_suggestion(file_path, error_code).ok_or_else(|| {
            format!("No MachineApplicable fix found for {error_code} in {file_path}")
        })?;

        let project_root = {
            let inner = self.inner.read();
            inner.project_root.clone()
        };

        // Resolve the file path (with path-traversal guard)
        let abs_path = safe_resolve_path(&project_root, &suggestion.file_path)
            .map_err(|e| format!("Path traversal blocked for {}: {e}", suggestion.file_path))?;

        let source = std::fs::read_to_string(&abs_path)
            .map_err(|e| format!("Failed to read {}: {e}", abs_path.display()))?;

        let lines: Vec<&str> = source.lines().collect();

        // Build the new content with the replacement applied
        let mut new_lines: Vec<String> = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            let line_num = i + 1; // 1-indexed

            if line_num == suggestion.line_start && suggestion.line_start == suggestion.line_end {
                // Single-line replacement
                let before = &line[..suggestion.column_start.saturating_sub(1)];
                let after = if suggestion.column_end <= line.len() {
                    &line[suggestion.column_end.saturating_sub(1)..]
                } else {
                    ""
                };
                new_lines.push(format!("{before}{}{after}", suggestion.replacement));
            } else if line_num < suggestion.line_start || line_num > suggestion.line_end {
                new_lines.push(line.to_string());
            } else if line_num == suggestion.line_start {
                // Multi-line: emit content before the span + replacement
                let before = &line[..suggestion.column_start.saturating_sub(1)];
                // Grab trailing content from the last line of the span
                let last_line = lines
                    .get(suggestion.line_end.saturating_sub(1))
                    .unwrap_or(&"");
                let after = if suggestion.column_end <= last_line.len() {
                    &last_line[suggestion.column_end.saturating_sub(1)..]
                } else {
                    ""
                };
                new_lines.push(format!("{before}{}{after}", suggestion.replacement));
            }
            // Lines between start and end (exclusive) are skipped — replaced by above
        }

        let new_source = new_lines.join("\n");
        // Preserve trailing newline if original had one
        let new_source = if source.ends_with('\n') {
            format!("{new_source}\n")
        } else {
            new_source
        };

        // D35: Write the patched file
        std::fs::write(&abs_path, &new_source)
            .map_err(|e| format!("Failed to write {}: {e}", abs_path.display()))?;

        // D35: Verify the fix compiles. If not, restore the original content.
        let check = Command::new("cargo")
            .args(["check", "--quiet"])
            .current_dir(&project_root)
            .env("CARGO_TERM_COLOR", "never")
            .output();

        match check {
            Ok(output) if output.status.success() => {
                debug!(
                    file = %suggestion.file_path,
                    code = %error_code,
                    "quickfix applied and verified (D35)"
                );
                Ok(format!(
                    "Applied fix for {} in {}:{}:{} — {}\n\nSuggested commit trailer:\n  Co-authored-by: Synapseed <synapseed@noreply>",
                    error_code,
                    suggestion.file_path,
                    suggestion.line_start,
                    suggestion.column_start,
                    suggestion.message,
                ))
            }
            _ => {
                // Revert: restore original content
                warn!(
                    file = %suggestion.file_path,
                    code = %error_code,
                    "quickfix broke compilation — reverting (D35)"
                );
                std::fs::write(&abs_path, &source).map_err(|e| {
                    format!(
                        "CRITICAL: cannot revert {}: {e} — file may be corrupted",
                        abs_path.display()
                    )
                })?;
                Err(format!(
                    "Fix for {error_code} in {} broke compilation — reverted to original",
                    suggestion.file_path
                ))
            }
        }
    }
}

/// Start the background check loop.
/// Listens for trigger signals and runs cargo check with adaptive debouncing.
///
/// HCI Req 4 (Focus Mode): If 3+ triggers arrive within 5 seconds, the debounce
/// window escalates from 2s to 5s — the user is in rapid edit flow.
pub fn start_background_loop(
    store: Arc<DiagnosticStore>,
    ctx: SynapseContext,
    trigger_rx: std::sync::mpsc::Receiver<()>,
) {
    let shutdown_flag = ctx.shutdown_flag();

    std::thread::spawn(move || {
        info!("Shadow: Background check loop started");

        // Run an initial check
        let (errors, warnings) = store.run_check();
        ctx.broadcast(SynapseEvent::DiagnosticUpdated { errors, warnings });

        // Adaptive debounce parameters
        let normal_debounce = Duration::from_secs(2);
        let rapid_debounce = Duration::from_secs(5);
        let rapid_window = Duration::from_secs(5);
        let rapid_threshold = 3usize;
        let mut recent_triggers: Vec<Instant> = Vec::new();

        loop {
            if shutdown_flag.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }

            // Wait for a trigger with timeout so we can check shutdown periodically
            match trigger_rx.recv_timeout(Duration::from_secs(2)) {
                Ok(()) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }

            if shutdown_flag.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }

            let now = Instant::now();
            recent_triggers.push(now);

            // Prune old triggers outside the rapid detection window
            recent_triggers.retain(|t| now.duration_since(*t) < rapid_window);

            // Adaptive debounce: escalate if rapid editing detected
            let debounce = if recent_triggers.len() >= rapid_threshold {
                debug!(
                    triggers = recent_triggers.len(),
                    "Shadow: Rapid editing detected, escalating debounce"
                );
                rapid_debounce
            } else {
                normal_debounce
            };

            // Drain any additional triggers (debounce)
            let deadline = Instant::now() + debounce;
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                match trigger_rx.recv_timeout(remaining) {
                    Ok(()) => {
                        recent_triggers.push(Instant::now());
                        continue;
                    }
                    Err(_) => break,
                }
            }

            if shutdown_flag.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }

            // Run the check
            let (errors, warnings) = store.run_check();
            ctx.broadcast(SynapseEvent::DiagnosticUpdated { errors, warnings });
        }

        info!("Shadow: Background check loop exited");
    });
}

// ── D33: Disk management ────────────────────────────────────────────────

/// Clean up the shadow target directory if it exceeds age or size limits.
///
/// - If the directory's modification time is older than [`SHADOW_MAX_AGE_SECS`],
///   it is removed entirely (cargo will rebuild incrementally).
/// - If total size exceeds [`SHADOW_MAX_SIZE_BYTES`], it is removed.
///
/// Errors are logged and swallowed — cleanup is best-effort.
fn cleanup_shadow_target(path: &std::path::Path) {
    if !path.exists() {
        return;
    }

    // Age check: if directory modification time exceeds max age, wipe it.
    if let Ok(meta) = std::fs::metadata(path) {
        if let Ok(modified) = meta.modified() {
            if let Ok(age) = modified.elapsed() {
                if age.as_secs() > SHADOW_MAX_AGE_SECS {
                    info!(
                        path = %path.display(),
                        age_days = age.as_secs() / 86400,
                        "Shadow: target dir exceeded max age, removing (D33)"
                    );
                    if let Err(e) = std::fs::remove_dir_all(path) {
                        warn!(error = %e, "Shadow: failed to remove stale target dir");
                    }
                    return;
                }
            }
        }
    }

    // Size check: walk the directory tree and sum file sizes.
    let total_bytes = dir_size(path);
    if total_bytes > SHADOW_MAX_SIZE_BYTES {
        info!(
            path = %path.display(),
            size_mb = total_bytes / (1024 * 1024),
            "Shadow: target dir exceeded max size, removing (D33)"
        );
        if let Err(e) = std::fs::remove_dir_all(path) {
            warn!(error = %e, "Shadow: failed to remove oversized target dir");
        }
    }
}

/// Recursively compute total size of a directory in bytes.
fn dir_size(path: &std::path::Path) -> u64 {
    let mut total: u64 = 0;
    let walker = match std::fs::read_dir(path) {
        Ok(w) => w,
        Err(_) => return 0,
    };
    for entry in walker.flatten() {
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if ft.is_file() {
            total += entry.metadata().map(|m| m.len()).unwrap_or(0);
        } else if ft.is_dir() {
            total += dir_size(&entry.path());
        }
    }
    total
}
