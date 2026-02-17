//! Atomic context builder — Semantic Ballast for sub-3B models.
//!
//! Design principles:
//! - NO JSON, NO `**bold**`, NO `##` headers — flat Markdown only
//! - `ENVIRONMENT:` header anchors the model to the real project
//! - `@@@ START_OF_TRUTH: path @@@` delimiters are unambiguous for tiny models
//! - Language reinforcement every ~10 lines inside injected code
//! - Raw source injection is always forced (even if user didn't request it)
//!
//! Split from context_builder (#64).

use super::{build_human_summary, detect_predominant_language};
use crate::router::{CodeContext, DiagnosticsContext, RawSource};

// ── Atomic Context Builder (Semantic Ballast v3.7.0) ───────────────────

pub(super) fn build_atomic_context(
    query: &str,
    intent_label: &str,
    code_context: &Option<CodeContext>,
    diagnostics: &Option<DiagnosticsContext>,
    raw_injection: bool,
    raw_sources: &[RawSource],
    project_root: &str,
) -> String {
    let mut parts = Vec::new();

    // Detect language from symbols
    let lang = detect_predominant_language(code_context)
        .unwrap_or_else(|| "unknown".to_string());

    // Environment header: ground the model in reality
    parts.push(format!(
        "ENVIRONMENT: This is a {lang} project. Files are located in {project_root}."
    ));
    parts.push(format!("TASK: {query} ({intent_label})"));

    // v4.17.1 (W7): Show compiler errors to atomic-tier models too
    if let Some(diag) = diagnostics {
        if diag.error_count > 0 || diag.warning_count > 0 {
            parts.push(format!(
                "COMPILER: {} error(s), {} warning(s)",
                diag.error_count, diag.warning_count
            ));
            for item in diag.items.iter().take(5) {
                let file = item.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                let line = item.get("line_start").and_then(|v| v.as_u64()).unwrap_or(0);
                let msg = item.get("message").and_then(|v| v.as_str()).unwrap_or("");
                if !msg.is_empty() {
                    parts.push(format!("  error: {file}:{line}: {msg}"));
                }
            }
        }
    }
    parts.push(String::new());

    // Human-readable symbol summary
    if let Some(summary) = build_human_summary(code_context) {
        parts.push(summary);
        parts.push(String::new());
    }

    // Raw source injection with unambiguous delimiters for sub-3B models
    if raw_injection && !raw_sources.is_empty() {
        let file_list: Vec<&str> = raw_sources.iter().map(|s| s.file_path.as_str()).collect();

        parts.push(format!("REAL SOURCE CODE FROM THIS {lang} PROJECT:"));
        parts.push(String::new());

        for src in raw_sources {
            if src.line_start == 0 && src.line_end == 0 {
                parts.push(format!("@@@ START_OF_TRUTH: {} (UNAVAILABLE) @@@", src.file_path));
                parts.push(src.source.clone());
                parts.push("@@@ END_OF_TRUTH @@@".into());
            } else {
                parts.push(format!(
                    "@@@ START_OF_TRUTH: {} (lines {}-{}) @@@",
                    src.file_path, src.line_start, src.line_end
                ));

                // Language reinforcement: insert a reminder every ~10 lines
                let source_lines: Vec<&str> = src.source.lines().collect();
                let mut reinforced = String::new();
                for (i, line) in source_lines.iter().enumerate() {
                    reinforced.push_str(line);
                    reinforced.push('\n');
                    if (i + 1) % 10 == 0 && i + 1 < source_lines.len() {
                        reinforced.push_str(&format!(
                            "// [SYNAPSEED: This is {lang} code from {}]\n",
                            src.file_path
                        ));
                    }
                }
                parts.push(reinforced.trim_end().to_string());
                parts.push("@@@ END_OF_TRUTH @@@".into());
            }
            parts.push(String::new());
        }

        // Instruction sandwiching: repeat grounding rules after code injection
        parts.push(format!(
            "INSTRUCTIONS: Answer using ONLY the {lang} source code above. \
             This project uses {lang}. Cite file paths and line numbers. \
             Do NOT reference files that are not listed above. \
             Do NOT use any other programming language."
        ));
        // Zero-Hallucination Recency Bias Guard: LAST line of context
        parts.push(format!(
            "IF YOU CITE A FILE NOT LISTED BELOW, YOU FAIL.\n\
             ALLOWED FILES: {}",
            file_list.join(", ")
        ));
    } else {
        parts.push(format!(
            "This is a {lang} project. Provide a concise answer about {lang} code."
        ));
    }

    parts.join("\n")
}
