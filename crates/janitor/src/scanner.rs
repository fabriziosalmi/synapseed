use std::path::Path;
use std::process::Command;

use tracing::debug;

/// A single issue found by `cargo clippy`.
#[derive(Debug, Clone)]
pub struct ClippyIssue {
    /// Lint code, e.g. "clippy::manual_map" or "unused_variables".
    pub lint_code: String,
    /// Human-readable message.
    pub message: String,
    /// Severity level: "warning" or "error".
    pub level: String,
    /// File path relative to the project root.
    pub file_path: String,
    /// Start line in the source file.
    pub line_start: u32,
    /// End line in the source file.
    pub line_end: u32,
    /// Machine-applicable suggestions from the compiler.
    pub suggestions: Vec<Suggestion>,
}

/// A compiler-suggested fix extracted from clippy JSON output.
#[derive(Debug, Clone)]
pub struct Suggestion {
    /// Help message describing the fix (e.g. "try").
    pub message: String,
    /// The replacement code.
    pub replacement: String,
    /// Byte offset where the replacement starts (within the file).
    pub byte_start: usize,
    /// Byte offset where the replacement ends (within the file).
    pub byte_end: usize,
    /// File path this suggestion applies to.
    pub file_path: String,
    /// Applicability: "MachineApplicable", "MaybeIncorrect", "HasPlaceholders", "Unspecified".
    pub applicability: String,
}

impl Suggestion {
    /// Whether this suggestion is safe to auto-apply.
    pub fn is_machine_applicable(&self) -> bool {
        self.applicability == "MachineApplicable"
    }
}

/// Scan a project with `cargo clippy --message-format=json` and collect issues.
pub fn scan_clippy(project_path: &Path) -> Result<Vec<ClippyIssue>, crate::JanitorError> {
    let output = Command::new("cargo")
        .args([
            "clippy",
            "--all-targets",
            "--message-format=json",
            "--quiet",
        ])
        .current_dir(project_path)
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .map_err(|e| {
            crate::JanitorError::Scanner(format!("Failed to run cargo clippy: {e}"))
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut issues = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for line in stdout.lines() {
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        // Only process compiler-message entries
        if msg.get("reason").and_then(|r| r.as_str()) != Some("compiler-message") {
            continue;
        }

        let Some(message) = msg.get("message") else {
            continue;
        };

        // Extract lint code
        let lint_code = message
            .get("code")
            .and_then(|c| c.get("code"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        // Skip if no lint code (build script output, etc.)
        if lint_code.is_empty() {
            continue;
        }

        let level = message
            .get("level")
            .and_then(|l| l.as_str())
            .unwrap_or("unknown")
            .to_string();

        // Only process warnings and errors
        if level != "warning" && level != "error" {
            continue;
        }

        let msg_text = message
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();

        // Extract primary span info
        let primary_span = message
            .get("spans")
            .and_then(|s| s.as_array())
            .and_then(|spans| spans.iter().find(|s| s.get("is_primary") == Some(&serde_json::Value::Bool(true))));

        let (file_path, line_start, line_end) = match primary_span {
            Some(span) => (
                span.get("file_name")
                    .and_then(|f| f.as_str())
                    .unwrap_or("")
                    .to_string(),
                span.get("line_start")
                    .and_then(|l| l.as_u64())
                    .unwrap_or(0) as u32,
                span.get("line_end")
                    .and_then(|l| l.as_u64())
                    .unwrap_or(0) as u32,
            ),
            None => continue,
        };

        // Deduplicate by (lint_code, file_path, line_start)
        let dedup_key = format!("{lint_code}:{file_path}:{line_start}");
        if !seen.insert(dedup_key) {
            continue;
        }

        // Extract suggestions from children
        let suggestions = extract_suggestions(message);

        issues.push(ClippyIssue {
            lint_code,
            message: msg_text,
            level,
            file_path,
            line_start,
            line_end,
            suggestions,
        });
    }

    debug!(
        issues_found = issues.len(),
        fixable = issues.iter().filter(|i| i.has_auto_fix()).count(),
        "Clippy scan complete"
    );

    Ok(issues)
}

impl ClippyIssue {
    /// Whether this issue has at least one MachineApplicable suggestion.
    pub fn has_auto_fix(&self) -> bool {
        self.suggestions.iter().any(|s| s.is_machine_applicable())
    }

    /// Get the first MachineApplicable suggestion, if any.
    pub fn auto_fix(&self) -> Option<&Suggestion> {
        self.suggestions
            .iter()
            .find(|s| s.is_machine_applicable())
    }
}

/// Extract suggestions from `message.children[].spans[]` in clippy JSON output.
fn extract_suggestions(message: &serde_json::Value) -> Vec<Suggestion> {
    let mut suggestions = Vec::new();

    let Some(children) = message.get("children").and_then(|c| c.as_array()) else {
        return suggestions;
    };

    for child in children {
        let child_msg = child
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();

        let Some(spans) = child.get("spans").and_then(|s| s.as_array()) else {
            continue;
        };

        for span in spans {
            let Some(replacement) = span
                .get("suggested_replacement")
                .and_then(|r| r.as_str())
            else {
                continue;
            };

            let applicability = span
                .get("suggestion_applicability")
                .and_then(|a| a.as_str())
                .unwrap_or("Unspecified")
                .to_string();

            let file_path = span
                .get("file_name")
                .and_then(|f| f.as_str())
                .unwrap_or("")
                .to_string();

            let byte_start = span
                .get("byte_start")
                .and_then(|b| b.as_u64())
                .unwrap_or(0) as usize;

            let byte_end = span
                .get("byte_end")
                .and_then(|b| b.as_u64())
                .unwrap_or(0) as usize;

            suggestions.push(Suggestion {
                message: child_msg.clone(),
                replacement: replacement.to_string(),
                byte_start,
                byte_end,
                file_path,
                applicability,
            });
        }
    }

    suggestions
}

/// Check for potentially unused dependencies by scanning Cargo.toml
/// vs `use` statements in source files.
///
/// Returns crate names that appear in `[dependencies]` but are never
/// imported via `use <crate>::`, `extern crate <crate>`, or `<crate>::`.
pub fn scan_unused_deps(project_path: &Path) -> Result<Vec<String>, crate::JanitorError> {
    let cargo_toml = project_path.join("Cargo.toml");
    let content = std::fs::read_to_string(&cargo_toml).map_err(|e| {
        crate::JanitorError::Scanner(format!("Cannot read Cargo.toml: {e}"))
    })?;

    // Parse dependency names from [dependencies] section
    let dep_names = parse_dependency_names(&content);

    if dep_names.is_empty() {
        return Ok(Vec::new());
    }

    // Collect all Rust source content
    let src_dir = project_path.join("src");
    let mut all_source = String::new();
    collect_rust_sources(&src_dir, &mut all_source);

    // Check each dependency for usage
    let mut unused = Vec::new();
    for dep in &dep_names {
        // Normalize crate name: hyphens become underscores in Rust imports
        let import_name = dep.replace('-', "_");

        let used = all_source.contains(&format!("use {import_name}"))
            || all_source.contains(&format!("{import_name}::"))
            || all_source.contains(&format!("extern crate {import_name}"));

        if !used {
            unused.push(dep.clone());
        }
    }

    if !unused.is_empty() {
        debug!(count = unused.len(), "Found potentially unused dependencies");
    }

    Ok(unused)
}

/// Parse dependency names from the `[dependencies]` section of Cargo.toml.
fn parse_dependency_names(cargo_toml: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_deps = false;

    for line in cargo_toml.lines() {
        let trimmed = line.trim();

        if trimmed == "[dependencies]" {
            in_deps = true;
            continue;
        }

        // End of [dependencies] section
        if trimmed.starts_with('[') {
            in_deps = false;
            continue;
        }

        if in_deps {
            // Parse "name = ..." or "name = { ... }"
            if let Some(eq_pos) = trimmed.find('=') {
                let name = trimmed[..eq_pos].trim().to_string();
                if !name.is_empty() && !name.starts_with('#') {
                    names.push(name);
                }
            }
        }
    }

    names
}

/// Recursively collect all `.rs` file contents into a single string.
fn collect_rust_sources(dir: &Path, buf: &mut String) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rust_sources(&path, buf);
        } else if path.extension().is_some_and(|e| e == "rs") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                buf.push_str(&content);
                buf.push('\n');
            }
        }
    }
}
