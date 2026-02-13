//! Context building — assembles the smart_context string for LLM consumption.
//!
//! Three strategies by model tier:
//! - **Atomic** (<3B): Semantic Ballast — flat text, `@@@ START_OF_TRUTH` delimiters,
//!   language reinforcement every 10 lines, zero-hallucination guard.
//! - **Molecular** (3-13B): Structured sections, phase-aware preamble.
//! - **Galactic** (13B+): Rich Markdown, cross-referenced analysis.

use tracing::debug;

use synapseed_core::context::SynapseContext;
use synapseed_core::momentum::{ModelTier, SessionPhase};
use synapseed_cortex::graph::CodeGraph;

use super::{CodeContext, DiagnosticsContext, HistoryContext, Intent, QueryComplexity, RawSource, Target};

// ── Smart Context Input ────────────────────────────────────────────────

/// Input context for the smart context builder.
pub(super) struct SmartContextInput<'a> {
    pub query: &'a str,
    pub intent: &'a Intent,
    pub complexity: QueryComplexity,
    pub diagnostics: &'a Option<DiagnosticsContext>,
    pub histories: &'a [HistoryContext],
    pub code_context: &'a Option<CodeContext>,
    pub security_status: &'a str,
    pub raw_injection: bool,
    pub raw_sources: &'a [RawSource],
    pub tier: ModelTier,
    pub phase: SessionPhase,
    pub project_root: String,
}

// ── Human Summary Builder ─────────────────────────────────────────────

/// Build a one-line-per-symbol summary for small LLMs that struggle with
/// raw JSON.  Output example:
///
/// ```text
/// Found fn `ask(query: &str, ctx: &SynapseContext) -> WhisperResult` in crates/whisper/src/router/mod.rs at line 136
/// Found struct `WhisperResult` in crates/whisper/src/router/mod.rs at line 81
/// ```
pub(super) fn build_human_summary(code_context: &Option<CodeContext>) -> Option<String> {
    let code = code_context.as_ref()?;
    if code.symbols.is_empty() {
        return None;
    }

    let lines: Vec<String> = code
        .symbols
        .iter()
        .filter_map(|sym| {
            let name = sym.get("name")?.as_str()?;
            let file = sym.get("file_path")?.as_str()?;
            let line = sym.get("line_start")?.as_u64()?;
            let kind = sym
                .get("kind")
                .and_then(|k| k.as_str())
                .unwrap_or("symbol");
            // Include signature if available (v3.6.2: Narrative Bridge)
            let display_name = if let Some(sig) = sym.get("signature").and_then(|s| s.as_str()) {
                sig.to_string()
            } else {
                format!("`{name}`")
            };
            Some(format!("Found {kind} {display_name} in {file} at line {line}"))
        })
        .collect();

    if lines.is_empty() {
        return None;
    }

    Some(lines.join("\n"))
}

/// Detect the predominant language across discovered symbols.
fn detect_predominant_language(code_context: &Option<CodeContext>) -> Option<String> {
    let code = code_context.as_ref()?;
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for sym in &code.symbols {
        if let Some(file) = sym.get("file_path").and_then(|f| f.as_str()) {
            let ext = file.rsplit('.').next().unwrap_or("");
            let lang = match ext {
                "rs" => "Rust",
                "py" | "pyi" => "Python",
                "js" | "mjs" | "cjs" => "JavaScript",
                "ts" | "tsx" | "mts" => "TypeScript",
                "go" => "Go",
                "java" => "Java",
                "c" | "h" => "C",
                "cpp" | "cc" | "cxx" | "hpp" => "C++",
                "rb" => "Ruby",
                "swift" => "Swift",
                "kt" | "kts" => "Kotlin",
                _ => ext,
            };
            *counts.entry(lang).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(lang, _)| lang.to_string())
}

// ── Raw Source Injection ────────────────────────────────────────────────

/// Read the actual source code for each target that has file/line info.
///
/// Minify source code to reduce token waste without losing semantics.
///
/// - Strips trailing whitespace from each line
/// - Collapses runs of 3+ blank lines into a single blank line
fn minify_source(source: &str) -> String {
    let mut result = Vec::new();
    let mut blank_run = 0;

    for line in source.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            blank_run += 1;
            if blank_run <= 1 {
                result.push("");
            }
            // Runs of 2+ blanks → collapsed to 1
        } else {
            blank_run = 0;
            result.push(trimmed);
        }
    }

    result.join("\n")
}

/// Prune non-structural noise from source code (v4.7.0 — "La Dieta del Token").
///
/// Collapses logging/debug statements into a single comment and truncates
/// overly long lines (string constants, generated code). Preserves all structural
/// code (function signatures, control flow, return values) intact.
///
/// Savings: typically 10-30% token reduction on real-world code.
fn prune_noise(source: &str, file_ext: &str) -> String {
    let comment = match file_ext {
        "py" | "pyi" | "rb" | "sh" | "bash" | "yaml" | "yml" | "toml" => "#",
        _ => "//",
    };

    let mut result = Vec::new();
    let mut in_log_block = false;
    let mut paren_depth: i32 = 0;
    let mut last_was_pruned = false;

    for line in source.lines() {
        let trimmed = line.trim();

        // Track multi-line log blocks (e.g., debug!(\n  field = value,\n  "msg"\n);)
        if in_log_block {
            paren_depth += trimmed.chars().filter(|&c| c == '(').count() as i32;
            paren_depth -= trimmed.chars().filter(|&c| c == ')').count() as i32;
            if paren_depth <= 0 {
                in_log_block = false;
                paren_depth = 0;
            }
            continue; // swallow line
        }

        // Detect logging statements
        if is_log_statement(trimmed) {
            paren_depth = trimmed.chars().filter(|&c| c == '(').count() as i32
                - trimmed.chars().filter(|&c| c == ')').count() as i32;
            if paren_depth > 0 {
                in_log_block = true;
            }
            if !last_was_pruned {
                let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
                result.push(format!("{indent}{comment} ..."));
            }
            last_was_pruned = true;
            continue;
        }

        last_was_pruned = false;

        // Truncate overly long lines (string constants, generated code)
        if line.len() > 200 {
            // Safe truncation: find nearest char boundary before 200
            let end = line.floor_char_boundary(200);
            result.push(format!("{}...", &line[..end]));
        } else {
            result.push(line.to_string());
        }
    }

    result.join("\n")
}

/// Detect logging/debug statements across common languages.
///
/// Covers Rust (tracing, log, std), Python (logging, logger, print),
/// and JavaScript/TypeScript (console.*).
fn is_log_statement(trimmed: &str) -> bool {
    // Rust: macro-based logging
    for prefix in [
        "debug!(", "info!(", "warn!(", "error!(", "trace!(",
        "println!(", "eprintln!(", "dbg!(",
        "tracing::debug!(", "tracing::info!(", "tracing::warn!(",
        "tracing::error!(", "tracing::trace!(",
        "log::debug!(", "log::info!(", "log::warn!(",
        "log::error!(", "log::trace!(",
    ] {
        if trimmed.starts_with(prefix) {
            return true;
        }
    }

    // Python: logger.method() / logging.method()
    for obj in ["logger.", "logging.", "log."] {
        if trimmed.starts_with(obj) {
            let after = &trimmed[obj.len()..];
            for method in ["debug(", "info(", "warning(", "error(", "critical(", "exception("] {
                if after.starts_with(method) {
                    return true;
                }
            }
        }
    }

    // Python: print() — almost always debug noise in framework code
    if trimmed.starts_with("print(") {
        return true;
    }

    // JavaScript/TypeScript: console.*()
    if trimmed.starts_with("console.") {
        let after = &trimmed["console.".len()..];
        for method in ["log(", "error(", "warn(", "debug(", "info(", "trace("] {
            if after.starts_with(method) {
                return true;
            }
        }
    }

    false
}

/// When `atomic_mode` is true (Semantic Ballast), the budget is doubled and
/// each snippet is expanded to at least 30 lines to give small models enough
/// grounding context.
pub(super) fn inject_raw_sources(targets: &[Target], ctx: &SynapseContext, atomic_mode: bool) -> Vec<RawSource> {
    let char_budget: usize = if atomic_mode { 32_000 } else { 16_000 };
    let min_lines: usize = if atomic_mode { 30 } else { 0 };
    let root = ctx.project_root();
    let mut sources = Vec::new();
    let mut budget_used: usize = 0;

    // Sort targets by score DESC so the most relevant symbols get budget priority.
    // Targets without scores (from non-search passes) go last.
    let mut sorted_targets: Vec<&Target> = targets.iter().collect();
    sorted_targets.sort_by(|a, b| {
        let sa = a.score.unwrap_or(0.0);
        let sb = b.score.unwrap_or(0.0);
        sb.total_cmp(&sa)
    });

    // Retrieve the code graph from the context for precise line range lookup
    let graph = ctx.get_extension::<CodeGraph>();

    for target in &sorted_targets {
        let rel_path = match &target.file_path {
            Some(p) => p.clone(),
            None => continue,
        };

        let abs_path = root.join(&rel_path);
        let content = match std::fs::read_to_string(&abs_path) {
            Ok(c) => c,
            Err(e) => {
                debug!(path = %abs_path.display(), error = %e, "Whisper: Could not read file for raw injection");
                // Inject an explicit error so the LLM knows data is missing
                sources.push(RawSource {
                    file_path: rel_path,
                    line_start: 0,
                    line_end: 0,
                    source: format!("[ERROR: Could not read file — {e}]"),
                });
                continue;
            }
        };

        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            continue;
        }

        // Try to get precise line range from Cortex (Symbol ID or Name lookup)
        let (start, end) = if let Some(g) = &graph {
            let candidates = g.lookup(&target.name);
            let sym = candidates.iter().find(|s| s.file_path.ends_with(&rel_path));

            if let Some(s) = sym {
                (s.line_start, s.line_end)
            } else if let Some(ls) = target.line_start {
                let s = ls.saturating_sub(1);
                let e = (ls + 30).min(lines.len());
                (s + 1, e)
            } else {
                (1, lines.len().min(60))
            }
        } else if let Some(ls) = target.line_start {
            // Fallback if no graph: ±30 lines around the target
            let s = ls.saturating_sub(1);
            let e = (ls + 30).min(lines.len());
            (s + 1, e) // 1-indexed
        } else {
            // No line info and no graph — take first 60 lines
            (1, lines.len().min(60))
        };

        // Clamp to file bounds (1-indexed)
        let s = start.max(1).min(lines.len());
        let mut e = end.max(s).min(lines.len());

        // Semantic Ballast: ensure at least min_lines per snippet
        if min_lines > 0 && (e - s + 1) < min_lines {
            e = (s + min_lines - 1).min(lines.len());
        }

        let snippet: String = lines[(s - 1)..e].join("\n");
        let snippet = minify_source(&snippet);

        // AST-based noise reduction (v4.7.0): prune logging, debug output,
        // and overly long lines to maximize context efficiency.
        let file_ext = rel_path.rsplit('.').next().unwrap_or("");
        let snippet = prune_noise(&snippet, file_ext);

        // Budget management: truncate oversized snippets instead of skipping.
        // For very large functions, a truncated view is better than nothing.
        let snippet = if budget_used + snippet.len() > char_budget {
            let remaining = char_budget.saturating_sub(budget_used);
            if remaining < 200 {
                // Budget nearly exhausted — skip remaining targets
                continue;
            }
            // Smart truncation: keep first half + last quarter of the budget slice
            // so the model sees both the function header and its tail.
            let first_portion = remaining * 3 / 4;
            let last_portion = remaining - first_portion - 30; // 30 chars for separator
            if last_portion > 50 && snippet.len() > remaining {
                let first_end = snippet.floor_char_boundary(first_portion);
                let last_start = snippet.floor_char_boundary(snippet.len() - last_portion);
                let mut truncated = snippet[..first_end].to_string();
                truncated.push_str("\n// ... [truncated] ...\n");
                truncated.push_str(&snippet[last_start..]);
                debug!(file = %rel_path, original = snippet.len(), truncated = truncated.len(), "Whisper: truncated oversized snippet");
                truncated
            } else {
                // Just take what fits
                let end = snippet.floor_char_boundary(remaining);
                snippet[..end].to_string()
            }
        } else {
            snippet
        };
        budget_used += snippet.len();

        sources.push(RawSource {
            file_path: rel_path,
            line_start: s,
            line_end: e,
            source: snippet,
        });
    }

    sources
}

// ── Git Staged Detection (#54) ──────────────────────────────────────────

/// Detect whether git has staged files in the working directory.
/// Sub-ms: spawns `git diff --cached --name-only` synchronously.
pub(super) fn detect_git_staged(ctx: &SynapseContext) -> bool {
    let root = ctx.project_root();
    match std::process::Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(&root)
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let has_staged = !stdout.trim().is_empty();
            if has_staged {
                debug!(files = stdout.trim(), "Git: staged files detected → forcing Stabilization");
            }
            has_staged
        }
        Err(e) => {
            debug!(error = %e, "Git: could not check staged files");
            false
        }
    }
}

// ── Atomic Context Builder (Semantic Ballast v3.7.0) ───────────────────

/// Build a fully-grounded context for Atomic tier models (<3B parameters).
///
/// Design principles:
/// - NO JSON, NO `**bold**`, NO `##` headers — flat Markdown only
/// - `ENVIRONMENT:` header anchors the model to the real project
/// - `@@@ START_OF_TRUTH: path @@@` delimiters are unambiguous for tiny models
/// - Language reinforcement every ~10 lines inside injected code
/// - Raw source injection is always forced (even if user didn't request it)
fn build_atomic_context(
    query: &str,
    intent_label: &str,
    code_context: &Option<CodeContext>,
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

// ── Smart Context Builder ──────────────────────────────────────────────

pub(super) fn build_smart_context(input: SmartContextInput) -> String {
    let query = input.query;
    let intent = input.intent;
    let complexity = input.complexity;
    let diagnostics = input.diagnostics;
    let histories = input.histories;
    let code_context = input.code_context;
    let security_status = input.security_status;
    let raw_injection = input.raw_injection;
    let raw_sources = input.raw_sources;
    let tier = input.tier;
    let phase = input.phase;
    let project_root = &input.project_root;

    let intent_label = match intent {
        Intent::BugFix => "bug fix",
        Intent::Security => "security audit",
        Intent::Explain => "code explanation",
        Intent::Refactor => "refactoring",
        Intent::General => "general inquiry",
    };

    // ── Atomic Tier: Semantic Ballast — grounded, flat output ──────
    if tier == ModelTier::Atomic {
        return build_atomic_context(
            query,
            intent_label,
            code_context,
            raw_injection,
            raw_sources,
            project_root,
        );
    }

    // ── Tier-Adapted Preamble (#51) ─────────────────────────────────
    let preamble = match tier {
        ModelTier::Atomic => unreachable!(), // handled above
        ModelTier::Molecular => match complexity {
            QueryComplexity::Quick => format!(
                "Brief answer for \"{query}\" ({intent_label}):"
            ),
            QueryComplexity::Standard => format!(
                "Query: \"{query}\" | Intent: {intent_label} | Phase: {phase}"
            ),
            QueryComplexity::Deep => format!(
                "Detailed analysis for \"{query}\" — intent: {intent_label}, phase: {phase}."
            ),
        },
        ModelTier::Galactic => match complexity {
            QueryComplexity::Quick => format!(
                "Brief answer for \"{query}\" ({intent_label}):"
            ),
            QueryComplexity::Standard => format!(
                "Based on your query \"{query}\", SYNAPSEED detected a **{intent_label}** intent (phase: **{phase}**) and gathered:"
            ),
            QueryComplexity::Deep => format!(
                "Detailed analysis for \"{query}\" — detected intent: **{intent_label}**, session phase: **{phase}**.\n\
                 SYNAPSEED gathered comprehensive context across all subsystems:"
            ),
        },
    };

    let mut parts = vec![preamble];

    // Language Pinning (v3.6.2): tell the model which language it's working with
    if let Some(lang) = detect_predominant_language(code_context) {
        parts.push(format!("WORKING_LANGUAGE: {lang}"));
    }

    // Human-readable symbol summary — helps small models locate answers fast
    if let Some(summary) = build_human_summary(code_context) {
        parts.push(summary);
    }

    // ── Molecular / Galactic: structured sections ──────────────────
    let mut section_count = 0usize;
    let max_sections = match complexity {
        QueryComplexity::Quick => 3,
        QueryComplexity::Standard => 5,
        QueryComplexity::Deep => usize::MAX,
    };

    if section_count < max_sections {
        if let Some(diag) = diagnostics {
            if diag.error_count > 0 || diag.warning_count > 0 {
                parts.push(format!(
                    "- **Compiler**: {} error(s), {} warning(s)",
                    diag.error_count, diag.warning_count
                ));
                // v4.12.0: Show actual diagnostic items so the LLM can see which errors exist.
                // Cap at 10 items to avoid overwhelming the context window.
                for item in diag.items.iter().take(10) {
                    let file = item.get("file").and_then(|v| v.as_str()).unwrap_or("?");
                    let line = item.get("line").and_then(|v| v.as_u64()).unwrap_or(0);
                    let level = item.get("level").and_then(|v| v.as_str()).unwrap_or("error");
                    let msg = item.get("message").and_then(|v| v.as_str()).unwrap_or("");
                    if !msg.is_empty() {
                        parts.push(format!("  {level}: {file}:{line}: {msg}"));
                    }
                }
            } else {
                parts.push("- **Compiler**: No errors or warnings".into());
            }
            section_count += 1;
        }
    }

    if section_count < max_sections {
        // v4.12.0: Multi-file history — show churn/risk for all target files.
        for hist in histories {
            // Chronos Sentiment Bridge (v3.6.2): include latest commit message
            let latest_msg = hist
                .recent_commits
                .first()
                .and_then(|c| c.get("message"))
                .and_then(|m| m.as_str())
                .map(|m| {
                    let truncated: String = m.chars().take(80).collect();
                    if m.len() > 80 { format!("{truncated}...") } else { truncated }
                });
            let mut hist_line = format!(
                "- **History** ({}): {} commit(s), hotspot {:.1}, risk: {}",
                hist.file, hist.total_commits, hist.hotspot_score, hist.risk
            );
            if let Some(msg) = latest_msg {
                hist_line.push_str(&format!("\n  Latest: \"{msg}\""));
            }
            parts.push(hist_line);
            if hist.rigidity > 0.5 {
                let effort = if hist.convergence_rate > 0.0 {
                    format!("{:.1}x", 1.0 / hist.convergence_rate)
                } else {
                    "high".to_string()
                };
                parts.push(format!(
                    "- **High Rigidity** ({}): {:.0}% — changes historically require ~{} the normal effort",
                    hist.file, hist.rigidity * 100.0, effort
                ));
            }
        }
        if !histories.is_empty() {
            section_count += 1;
        }
    }

    if section_count < max_sections {
        if let Some(code) = code_context {
            if code.symbols.is_empty() {
                parts.push("- **Code**: No matching symbols found for this query".into());
            } else {
                parts.push(format!(
                    "- **Code**: {} relevant symbol(s) found",
                    code.symbols.len()
                ));
            }
            section_count += 1;
        }
    }

    if section_count < max_sections {
        match security_status {
            "CLEAN" => parts.push("- **Security**: CLEAN".into()),
            "NOT_SCANNED" => {}
            status => parts.push(format!("- **Security**: {status}")),
        }
    }

    // ── Raw Source Injection block (v3.6.2: Path Anchoring) ──────
    if raw_injection && !raw_sources.is_empty() {
        parts.push(String::new());
        parts.push("## Injected Source Code".into());
        parts.push("You are provided with the EXACT source code for your query. \
                     Use the provided file paths and line numbers in your answer.".into());
        for src in raw_sources {
            if src.line_start == 0 && src.line_end == 0 {
                // I/O error placeholder (v3.6.2: Transparent I/O Errors)
                parts.push(format!(
                    "\n--- FILE: {} (UNAVAILABLE) ---",
                    src.file_path
                ));
                parts.push(src.source.clone());
            } else {
                parts.push(format!(
                    "\n--- FILE: {} (lines {}-{}) ---",
                    src.file_path, src.line_start, src.line_end
                ));
                parts.push(src.source.clone());
            }
            parts.push("--- END ---".into());
        }
    }

    if raw_injection && !raw_sources.is_empty() {
        // Instruction sandwiching: repeat grounding rules after code
        let file_list: Vec<&str> = raw_sources.iter().map(|s| s.file_path.as_str()).collect();
        parts.push("\nAnswer based ONLY on the injected source code above. \
             Cite exact file paths and line numbers. \
             ONLY use the file paths listed above. DO NOT invent file names.".to_string());
        // Zero-Hallucination Recency Bias Guard: LAST line of context
        parts.push(format!(
            "\nIF YOU CITE A FILE NOT LISTED BELOW, YOU FAIL.\n\
             ALLOWED FILES: {}",
            file_list.join(", ")
        ));
    } else {
        let closing = match complexity {
            QueryComplexity::Quick => "\nProvide a concise answer.",
            QueryComplexity::Standard => "\nUse the full JSON context below to provide an informed, precise answer.",
            QueryComplexity::Deep => "\nUse ALL gathered context to provide a thorough, cross-referenced analysis with specific file paths and line numbers.",
        };
        parts.push(closing.into());
    }
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapseed_core::momentum::{ModelTier, SessionPhase};

    // ── build_human_summary tests ────────────────────────────────────

    #[test]
    fn test_human_summary_none_when_no_context() {
        assert!(build_human_summary(&None).is_none());
    }

    #[test]
    fn test_human_summary_none_when_empty_symbols() {
        let ctx = CodeContext {
            symbols: vec![],
        };
        assert!(build_human_summary(&Some(ctx)).is_none());
    }

    #[test]
    fn test_human_summary_single_symbol() {
        let sym = serde_json::json!({
            "name": "ask",
            "kind": "function",
            "file_path": "crates/whisper/src/router/mod.rs",
            "line_start": 136
        });
        let ctx = CodeContext {
            symbols: vec![sym],
        };
        let summary = build_human_summary(&Some(ctx)).unwrap();
        assert_eq!(
            summary,
            "Found function `ask` in crates/whisper/src/router/mod.rs at line 136"
        );
    }

    #[test]
    fn test_human_summary_multiple_symbols() {
        let symbols = vec![
            serde_json::json!({
                "name": "ask",
                "kind": "function",
                "file_path": "crates/whisper/src/router/mod.rs",
                "line_start": 136
            }),
            serde_json::json!({
                "name": "WhisperResult",
                "kind": "struct",
                "file_path": "crates/whisper/src/router/mod.rs",
                "line_start": 81
            }),
        ];
        let ctx = CodeContext { symbols };
        let summary = build_human_summary(&Some(ctx)).unwrap();
        assert!(summary.contains("Found function `ask`"));
        assert!(summary.contains("Found struct `WhisperResult`"));
        assert_eq!(summary.lines().count(), 2);
    }

    #[test]
    fn test_human_summary_missing_fields_skipped() {
        let symbols = vec![
            serde_json::json!({ "name": "orphan" }), // missing file_path + line_start
            serde_json::json!({
                "name": "valid",
                "kind": "enum",
                "file_path": "src/lib.rs",
                "line_start": 10
            }),
        ];
        let ctx = CodeContext { symbols };
        let summary = build_human_summary(&Some(ctx)).unwrap();
        assert_eq!(summary.lines().count(), 1);
        assert!(summary.contains("Found enum `valid`"));
    }

    #[test]
    fn test_human_summary_fallback_kind() {
        let sym = serde_json::json!({
            "name": "mystery",
            "file_path": "src/lib.rs",
            "line_start": 1
        });
        let ctx = CodeContext {
            symbols: vec![sym],
        };
        let summary = build_human_summary(&Some(ctx)).unwrap();
        assert!(summary.contains("Found symbol `mystery`"));
    }

    #[test]
    fn test_human_summary_injected_in_smart_context() {
        let code_ctx = Some(CodeContext {
            symbols: vec![serde_json::json!({
                "name": "execute",
                "kind": "function",
                "file_path": "crates/root/src/executor.rs",
                "line_start": 32
            })],
        });
        let input = SmartContextInput {
            query: "explain execute",
            intent: &Intent::Explain,
            complexity: QueryComplexity::Standard,
            diagnostics: &None,
            histories: &[],
            code_context: &code_ctx,
            security_status: "CLEAN",
            raw_injection: false,
            raw_sources: &[],
            tier: ModelTier::Galactic,
            phase: SessionPhase::Discovery,
            project_root: "/test".into(),
        };
        let ctx = build_smart_context(input);
        // Human summary appears before the section bullets
        let summary_pos = ctx.find("Found function `execute`").unwrap();
        let code_section_pos = ctx.find("**Code**").unwrap();
        assert!(summary_pos < code_section_pos);
    }

    #[test]
    fn test_smart_context_with_raw_injection() {
        let raw_sources = vec![RawSource {
            file_path: "src/main.rs".to_string(),
            line_start: 1,
            line_end: 2,
            source: "fn main() {\n    println!(\"Hello\");\n}".to_string(),
        }];

        let input = SmartContextInput {
            query: "show main",
            intent: &Intent::Explain,
            complexity: QueryComplexity::Standard,
            diagnostics: &None,
            histories: &[],
            code_context: &None,
            security_status: "CLEAN",
            raw_injection: true,
            raw_sources: &raw_sources,
            tier: ModelTier::Galactic,
            phase: SessionPhase::Discovery,
            project_root: "/test".into(),
        };

        let ctx = build_smart_context(input);
        assert!(ctx.contains("## Injected Source Code"));
        assert!(ctx.contains("--- FILE: src/main.rs (lines 1-2) ---"));
        assert!(ctx.contains("fn main()"));
        assert!(ctx.contains("Answer based ONLY on the injected source code"));
        // Zero-Hallucination Recency Bias Guard
        assert!(ctx.contains("IF YOU CITE A FILE NOT LISTED BELOW, YOU FAIL"));
        assert!(ctx.contains("ALLOWED FILES: src/main.rs"));
    }

    // ── Tier-adapted output tests (#51) ─────────────────────────────

    #[test]
    fn test_atomic_tier_grounded_output() {
        let input = SmartContextInput {
            query: "explain main",
            intent: &Intent::Explain,
            complexity: QueryComplexity::Standard,
            diagnostics: &None,
            histories: &[],
            code_context: &None,
            security_status: "CLEAN",
            raw_injection: false,
            raw_sources: &[],
            tier: ModelTier::Atomic,
            phase: SessionPhase::Discovery,
            project_root: "/projects/myapp".into(),
        };
        let ctx = build_smart_context(input);
        // Atomic: no markdown formatting, no ** bold
        assert!(!ctx.contains("**"));
        // Semantic Ballast: environment header
        assert!(ctx.contains("ENVIRONMENT:"));
        assert!(ctx.contains("/projects/myapp"));
        assert!(ctx.contains("TASK: explain main"));
        assert!(ctx.contains("concise answer"));
    }

    #[test]
    fn test_atomic_tier_raw_injection_grounded() {
        let code_ctx = Some(CodeContext {
            symbols: vec![serde_json::json!({
                "name": "hello",
                "kind": "function",
                "file_path": "src/lib.rs",
                "line_start": 1
            })],
        });
        let raw_sources = vec![RawSource {
            file_path: "src/lib.rs".to_string(),
            line_start: 1,
            line_end: 5,
            source: "pub fn hello() {}".to_string(),
        }];
        let input = SmartContextInput {
            query: "explain hello",
            intent: &Intent::Explain,
            complexity: QueryComplexity::Standard,
            diagnostics: &None,
            histories: &[],
            code_context: &code_ctx,
            security_status: "CLEAN",
            raw_injection: true,
            raw_sources: &raw_sources,
            tier: ModelTier::Atomic,
            phase: SessionPhase::Discovery,
            project_root: "/test".into(),
        };
        let ctx = build_smart_context(input);
        // Semantic Ballast: grounded delimiters (@@@ for Atomic tier)
        assert!(ctx.contains("ENVIRONMENT: This is a Rust project"));
        assert!(ctx.contains("@@@ START_OF_TRUTH: src/lib.rs (lines 1-5) @@@"));
        assert!(ctx.contains("pub fn hello()"));
        assert!(ctx.contains("@@@ END_OF_TRUTH @@@"));
        assert!(ctx.contains("REAL SOURCE CODE"));
        // No "## Injected Source Code" header
        assert!(!ctx.contains("## Injected Source Code"));
        // Anti-hallucination instruction
        assert!(ctx.contains("Do NOT use any other programming language"));
        // Zero-Hallucination Recency Bias Guard
        assert!(ctx.contains("IF YOU CITE A FILE NOT LISTED BELOW, YOU FAIL"));
    }

    #[test]
    fn test_atomic_language_reinforcement() {
        // 15 lines of code → should get a reinforcement comment
        let source = (0..15)
            .map(|i| format!("let x{i} = {i};"))
            .collect::<Vec<_>>()
            .join("\n");
        let code_ctx = Some(CodeContext {
            symbols: vec![serde_json::json!({
                "name": "test",
                "kind": "function",
                "file_path": "src/main.rs",
                "line_start": 1
            })],
        });
        let raw_sources = vec![RawSource {
            file_path: "src/main.rs".to_string(),
            line_start: 1,
            line_end: 15,
            source,
        }];
        let input = SmartContextInput {
            query: "explain test",
            intent: &Intent::Explain,
            complexity: QueryComplexity::Standard,
            diagnostics: &None,
            histories: &[],
            code_context: &code_ctx,
            security_status: "CLEAN",
            raw_injection: true,
            raw_sources: &raw_sources,
            tier: ModelTier::Atomic,
            phase: SessionPhase::Discovery,
            project_root: "/test".into(),
        };
        let ctx = build_smart_context(input);
        // Should contain language reinforcement comment
        assert!(ctx.contains("[SYNAPSEED: This is Rust code from src/main.rs]"));
    }

    #[test]
    fn test_galactic_tier_includes_phase() {
        let input = SmartContextInput {
            query: "explain the router architecture in detail with cross references",
            intent: &Intent::Explain,
            complexity: QueryComplexity::Standard,
            diagnostics: &None,
            histories: &[],
            code_context: &None,
            security_status: "CLEAN",
            raw_injection: false,
            raw_sources: &[],
            tier: ModelTier::Galactic,
            phase: SessionPhase::Implementation,
            project_root: "/test".into(),
        };
        let ctx = build_smart_context(input);
        assert!(ctx.contains("**Implementation**"));
        assert!(ctx.contains("**code explanation**"));
    }

    #[test]
    fn test_molecular_tier_includes_phase() {
        let input = SmartContextInput {
            query: "explain the router module",
            intent: &Intent::Explain,
            complexity: QueryComplexity::Standard,
            diagnostics: &None,
            histories: &[],
            code_context: &None,
            security_status: "CLEAN",
            raw_injection: false,
            raw_sources: &[],
            tier: ModelTier::Molecular,
            phase: SessionPhase::Stabilization,
            project_root: "/test".into(),
        };
        let ctx = build_smart_context(input);
        assert!(ctx.contains("Phase: Stabilization"));
    }

    // ── Noise Reduction tests (v4.7.0 — "La Dieta del Token") ───────

    #[test]
    fn test_prune_rust_logging_single_line() {
        let source = r#"fn calculate(x: i32) -> i32 {
    debug!("Starting calculation with x={}", x);
    let result = x * 2;
    info!("Result: {}", result);
    result
}"#;
        let pruned = prune_noise(source, "rs");
        assert!(pruned.contains("fn calculate"));
        assert!(pruned.contains("let result = x * 2;"));
        assert!(pruned.contains("result"));
        assert!(!pruned.contains("debug!"));
        assert!(!pruned.contains("info!"));
        assert!(pruned.contains("// ..."));
    }

    #[test]
    fn test_prune_rust_multiline_logging() {
        let source = r#"fn process() {
    debug!(
        cs = cs,
        threshold = COHERENCE_THRESHOLD,
        max_clusters,
        "Coherence Gate: TRIGGERED"
    );
    let x = 42;
}"#;
        let pruned = prune_noise(source, "rs");
        assert!(pruned.contains("fn process()"));
        assert!(pruned.contains("let x = 42;"));
        assert!(!pruned.contains("cs = cs"));
        assert!(!pruned.contains("TRIGGERED"));
        assert!(pruned.contains("// ..."));
    }

    #[test]
    fn test_prune_python_logging() {
        let source = r#"def complex_calc(x):
    logger.info(f"Starting calculation with {x}")
    if x < 0:
        raise ValueError("negative")
    logging.debug("intermediate step")
    return x * 2"#;
        let pruned = prune_noise(source, "py");
        assert!(pruned.contains("def complex_calc"));
        assert!(pruned.contains("raise ValueError"));
        assert!(pruned.contains("return x * 2"));
        assert!(!pruned.contains("logger.info"));
        assert!(!pruned.contains("logging.debug"));
        assert!(pruned.contains("# ..."));
    }

    #[test]
    fn test_prune_javascript_console() {
        let source = r#"function handleRequest(req) {
    console.log("Received request:", req.url);
    const result = processRequest(req);
    console.error("Error:", result.error);
    return result;
}"#;
        let pruned = prune_noise(source, "js");
        assert!(pruned.contains("function handleRequest"));
        assert!(pruned.contains("const result = processRequest(req);"));
        assert!(pruned.contains("return result;"));
        assert!(!pruned.contains("console.log"));
        assert!(!pruned.contains("console.error"));
    }

    #[test]
    fn test_prune_consecutive_logs_single_marker() {
        let source = r#"fn init() {
    info!("Starting...");
    info!("Loading config...");
    info!("Connecting...");
    let db = connect();
}"#;
        let pruned = prune_noise(source, "rs");
        // 3 consecutive logs → only 1 "// ..." marker
        let marker_count = pruned.matches("// ...").count();
        assert_eq!(marker_count, 1, "Should collapse consecutive logs: {pruned}");
        assert!(pruned.contains("let db = connect();"));
    }

    #[test]
    fn test_prune_preserves_structural_code() {
        let source = r#"pub fn authenticate(creds: &Credentials) -> Result<Token, AuthError> {
    let user = find_user(&creds.username)?;
    if !verify_password(&user, &creds.password) {
        return Err(AuthError::InvalidPassword);
    }
    Ok(generate_token(&user))
}"#;
        let pruned = prune_noise(source, "rs");
        // No logging → no changes
        assert_eq!(pruned, source);
    }

    #[test]
    fn test_prune_long_line_truncation() {
        let long_line = format!("let msg = \"{}\";", "x".repeat(250));
        let source = format!("fn foo() {{\n    {long_line}\n    return 42;\n}}");
        let pruned = prune_noise(&source, "rs");
        assert!(pruned.contains("return 42;"));
        // The long line should be truncated
        assert!(!pruned.contains(&"x".repeat(250)));
        assert!(pruned.contains("..."));
    }

    #[test]
    fn test_prune_rust_println_and_dbg() {
        let source = r#"fn main() {
    println!("Debug output: {:?}", data);
    dbg!(&value);
    eprintln!("Warning: {}", msg);
    do_work();
}"#;
        let pruned = prune_noise(source, "rs");
        assert!(!pruned.contains("println!"));
        assert!(!pruned.contains("dbg!"));
        assert!(!pruned.contains("eprintln!"));
        assert!(pruned.contains("do_work();"));
    }

    #[test]
    fn test_prune_python_print() {
        let source = r#"def process(data):
    print(f"Processing {len(data)} items")
    result = transform(data)
    print("Done")
    return result"#;
        let pruned = prune_noise(source, "py");
        assert!(!pruned.contains("print("));
        assert!(pruned.contains("result = transform(data)"));
        assert!(pruned.contains("return result"));
    }

    #[test]
    fn test_is_log_statement_coverage() {
        // Rust
        assert!(is_log_statement("debug!(\"msg\");"));
        assert!(is_log_statement("tracing::info!(key = val, \"msg\");"));
        assert!(is_log_statement("log::warn!(\"msg\");"));
        assert!(is_log_statement("println!(\"hello\");"));
        assert!(is_log_statement("dbg!(value);"));

        // Python
        assert!(is_log_statement("logger.debug(\"msg\")"));
        assert!(is_log_statement("logging.error(\"msg\")"));
        assert!(is_log_statement("print(\"debug output\")"));

        // JavaScript
        assert!(is_log_statement("console.log(\"msg\");"));
        assert!(is_log_statement("console.error(\"msg\");"));

        // NOT logging
        assert!(!is_log_statement("let debug = true;"));
        assert!(!is_log_statement("fn info() {}"));
        assert!(!is_log_statement("logger_factory.create()"));
        assert!(!is_log_statement("result = console_app.run()"));
        assert!(!is_log_statement("return x * 2;"));
    }
}
