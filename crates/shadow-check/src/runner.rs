//! Background runner — spawns `cargo check` and manages the diagnostic store.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use tracing::{debug, info, warn};

use synapseed_core::context::SynapseContext;
use synapseed_core::event::SynapseEvent;

use crate::diagnostic::{
    parse_cargo_line, Applicability, Diagnostic, DiagnosticLevel, DiagnosticSnapshot, Suggestion,
};

/// The shared diagnostic store — thread-safe, accessible via `ctx.get_extension()`.
pub struct DiagnosticStore {
    inner: RwLock<StoreInner>,
}

struct StoreInner {
    diagnostics: Vec<Diagnostic>,
    error_count: usize,
    warning_count: usize,
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
                last_check_ms: 0,
                project_root,
            }),
        }
    }

    /// Get a snapshot of current diagnostics.
    pub fn snapshot(&self) -> DiagnosticSnapshot {
        let inner = self.inner.read().unwrap();
        DiagnosticSnapshot {
            diagnostics: inner.diagnostics.clone(),
            error_count: inner.error_count,
            warning_count: inner.warning_count,
            last_check_ms: inner.last_check_ms,
        }
    }

    /// Get diagnostics for a specific file.
    pub fn for_file(&self, file_path: &str) -> Vec<Diagnostic> {
        let inner = self.inner.read().unwrap();
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
        let inner = self.inner.read().unwrap();
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
            let inner = self.inner.read().unwrap();
            inner.project_root.clone()
        };

        let start = Instant::now();

        let output = Command::new("cargo")
            .args(["check", "--message-format=json", "--quiet"])
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

        let errors = all_diags
            .iter()
            .filter(|d| d.level == DiagnosticLevel::Error)
            .count();
        let warnings = all_diags
            .iter()
            .filter(|d| d.level == DiagnosticLevel::Warning)
            .count();

        // Update the store
        {
            let mut inner = self.inner.write().unwrap();
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
    pub fn apply_fix(&self, file_path: &str, error_code: &str) -> Result<String, String> {
        let (_diag, suggestion) = self.find_suggestion(file_path, error_code).ok_or_else(|| {
            format!("No MachineApplicable fix found for {error_code} in {file_path}")
        })?;

        let project_root = {
            let inner = self.inner.read().unwrap();
            inner.project_root.clone()
        };

        // Resolve the file path
        let abs_path = if Path::new(&suggestion.file_path).is_absolute() {
            PathBuf::from(&suggestion.file_path)
        } else {
            project_root.join(&suggestion.file_path)
        };

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

        std::fs::write(&abs_path, &new_source)
            .map_err(|e| format!("Failed to write {}: {e}", abs_path.display()))?;

        Ok(format!(
            "Applied fix for {} in {}:{}:{} — {}",
            error_code,
            suggestion.file_path,
            suggestion.line_start,
            suggestion.column_start,
            suggestion.message,
        ))
    }
}

/// Start the background check loop.
/// Listens for trigger signals and runs cargo check with debouncing.
pub fn start_background_loop(
    store: Arc<DiagnosticStore>,
    ctx: SynapseContext,
    trigger_rx: std::sync::mpsc::Receiver<()>,
) {
    std::thread::spawn(move || {
        info!("Shadow: Background check loop started");

        // Run an initial check
        let (errors, warnings) = store.run_check();
        ctx.broadcast(SynapseEvent::DiagnosticUpdated { errors, warnings });

        // Then wait for triggers with debouncing
        let debounce = Duration::from_secs(2);

        while trigger_rx.recv().is_ok() {
            // Drain any additional triggers (debounce)
            let deadline = Instant::now() + debounce;
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                match trigger_rx.recv_timeout(remaining) {
                    Ok(()) => continue, // More triggers — keep draining
                    Err(_) => break,    // Timeout — time to run
                }
            }

            // Run the check
            let (errors, warnings) = store.run_check();
            ctx.broadcast(SynapseEvent::DiagnosticUpdated { errors, warnings });
        }

        info!("Shadow: Background check loop exited");
    });
}
