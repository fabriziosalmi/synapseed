//! Context building — assembles the smart_context string for LLM consumption.
//!
//! Three strategies by model tier:
//! - **Atomic** (<3B): Semantic Ballast — flat text, `@@@ START_OF_TRUTH` delimiters,
//!   language reinforcement every 10 lines, zero-hallucination guard.
//! - **Molecular** (3-13B): Structured sections, phase-aware preamble.
//! - **Galactic** (13B+): Rich Markdown, cross-referenced analysis.
//!
//! Split into submodules (#64):
//! - `atomic.rs`    — Atomic tier builder (Semantic Ballast)
//! - `injection.rs` — Raw source injection + noise pruning

mod atomic;
pub(super) mod injection;

use synapseed_core::context::SynapseContext;
use synapseed_core::momentum::{ModelTier, SessionPhase};

use super::{
    CodeContext, DiagnosticsContext, GatheredContext, HistoryContext, Intent, QueryComplexity,
    RawSource, SessionState,
};

// Re-exports for parent module
pub(in crate::router) use injection::inject_raw_sources;

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
    /// Cognitive Ledger session hint (injected after language pinning).
    pub session_hint: Option<String>,
}

impl<'a> SmartContextInput<'a> {
    /// Construct from [`SessionState`] + [`GatheredContext`] (v5.0.1).
    ///
    /// Replaces the previous 14-field manual construction at the call site.
    pub fn from_session(
        query: &'a str,
        state: &'a SessionState,
        gathered: &'a GatheredContext,
        project_root: String,
    ) -> Self {
        Self {
            query,
            intent: &state.intent,
            complexity: state.complexity,
            diagnostics: &gathered.diagnostics,
            histories: &gathered.histories,
            code_context: &gathered.code_context,
            security_status: &gathered.security_status,
            raw_injection: state.effective_raw,
            raw_sources: &gathered.raw_sources,
            tier: state.tier,
            phase: state.phase,
            project_root,
            session_hint: state.session_hint.clone(),
        }
    }
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
            let kind = sym.get("kind").and_then(|k| k.as_str()).unwrap_or("symbol");
            // Include signature if available (v3.6.2: Narrative Bridge)
            let display_name = if let Some(sig) = sym.get("signature").and_then(|s| s.as_str()) {
                sig.to_string()
            } else {
                format!("`{name}`")
            };
            let mut result = format!("Found {kind} {display_name} in {file} at line {line}");

            // v4.27.0 Body Enrichment: extract member names and body details
            // from the snippet field injected by gather_code_context.
            if let Some(snippet) = sym.get("snippet").and_then(|s| s.as_str()) {
                let kind_lower = kind.to_lowercase();
                match kind_lower.as_str() {
                    "interface" => {
                        if let Some(members) = extract_method_names(snippet) {
                            result.push_str(&format!("\n  Methods: {members}"));
                        }
                    }
                    "struct" | "class" => {
                        if let Some(fields) = extract_field_names(snippet) {
                            result.push_str(&format!("\n  Fields: {fields}"));
                        }
                    }
                    "enum" => {
                        if let Some(variants) = extract_field_names(snippet) {
                            result.push_str(&format!("\n  Variants: {variants}"));
                        }
                    }
                    "function" | "method" => {
                        if let Some(preview) = extract_body_preview(snippet) {
                            result.push_str(&format!("\n  Body: {preview}"));
                        }
                    }
                    "constant" => {
                        // Metadata files (Cargo.toml, etc.): show key facts
                        if let Some(preview) = extract_body_preview(snippet) {
                            result.push_str(&format!("\n  Content: {preview}"));
                        }
                    }
                    _ => {}
                }
            }

            Some(result)
        })
        .collect();

    if lines.is_empty() {
        return None;
    }

    Some(lines.join("\n"))
}

/// Extract method/function names from a trait/interface body snippet.
fn extract_method_names(snippet: &str) -> Option<String> {
    let mut methods = Vec::new();
    for line in snippet.lines() {
        let trimmed = line.trim();
        let fn_idx = if let Some(i) = trimmed.find("fn ") {
            i + 3
        } else {
            continue;
        };
        let after_fn = &trimmed[fn_idx..];
        let name: String = after_fn
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() && !methods.contains(&name) {
            methods.push(name);
        }
    }
    if methods.is_empty() {
        None
    } else {
        Some(methods.join(", "))
    }
}

/// Extract field names from a struct/enum body snippet.
fn extract_field_names(snippet: &str) -> Option<String> {
    let mut fields = Vec::new();
    for line in snippet.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.starts_with("#[")
            || trimmed.starts_with("pub struct")
            || trimmed.starts_with("struct")
            || trimmed.starts_with("pub enum")
            || trimmed.starts_with("enum")
            || trimmed.starts_with("pub class")
            || trimmed.starts_with("class")
            || trimmed == "{"
            || trimmed == "}"
        {
            continue;
        }
        let field_part = trimmed.strip_prefix("pub ").unwrap_or(trimmed);
        // Struct field: `name: Type,` or enum variant: `Name,` / `Name {`
        if let Some(colon_idx) = field_part.find(':') {
            let name: String = field_part[..colon_idx]
                .trim()
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() && !fields.contains(&name) {
                fields.push(name);
            }
        } else {
            // Enum variant without fields: `VariantName,` or `VariantName`
            let name: String = field_part
                .trim_end_matches(',')
                .trim()
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() && name != "}" && !fields.contains(&name) {
                fields.push(name);
            }
        }
    }
    if fields.is_empty() {
        None
    } else {
        Some(fields.join(", "))
    }
}

/// Extract a compact body preview from a function/method snippet.
/// Takes the first N significant lines (non-comment, non-brace),
/// concatenated with ` | ` and capped at 300 characters.
fn extract_body_preview(snippet: &str) -> Option<String> {
    let mut significant: Vec<&str> = Vec::new();
    let mut total_len = 0;
    const MAX_CHARS: usize = 400;
    const MAX_LINES: usize = 12;

    for line in snippet.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.starts_with("///")
            || trimmed == "{"
            || trimmed == "}"
            || trimmed == "};"
        {
            continue;
        }
        if significant.len() >= MAX_LINES || total_len + trimmed.len() > MAX_CHARS {
            break;
        }
        significant.push(trimmed);
        total_len += trimmed.len() + 3;
    }

    if significant.is_empty() {
        None
    } else {
        Some(significant.join(" | "))
    }
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
                tracing::debug!(
                    files = stdout.trim(),
                    "Git: staged files detected → forcing Stabilization"
                );
            }
            has_staged
        }
        Err(e) => {
            tracing::debug!(error = %e, "Git: could not check staged files");
            false
        }
    }
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

    // ── Semantic Ballast: grounded, flat output for tiny models ────
    if tier.needs_semantic_ballast() {
        return atomic::build_atomic_context(
            query,
            intent_label,
            code_context,
            diagnostics,
            raw_injection,
            raw_sources,
            project_root,
        );
    }

    // ── Tier-Adapted Preamble (#51, v5.0) ────────────────────────────
    // Molecular: concise structured.  Galactic+Universal: rich cross-referenced.
    let preamble = match tier {
        ModelTier::Atomic => unreachable!(), // handled by needs_semantic_ballast()
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
        ModelTier::Galactic | ModelTier::Universal => match complexity {
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

    // Cognitive Ledger: session hint breadcrumb (v4.19.0)
    if let Some(ref hint) = input.session_hint {
        parts.push(hint.clone());
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
                    // v4.17.1 (W7): Use correct Diagnostic struct field names:
                    // file_path (not "file"), line_start (not "line")
                    let file = item
                        .get("file_path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");
                    let line = item.get("line_start").and_then(|v| v.as_u64()).unwrap_or(0);
                    let level = item
                        .get("level")
                        .and_then(|v| v.as_str())
                        .unwrap_or("error");
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
                    if m.len() > 80 {
                        format!("{truncated}...")
                    } else {
                        truncated
                    }
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
        parts.push(
            "You are provided with the EXACT source code for your query. \
                     Use the provided file paths and line numbers in your answer."
                .into(),
        );
        for src in raw_sources {
            if src.line_start == 0 && src.line_end == 0 {
                // I/O error placeholder (v3.6.2: Transparent I/O Errors)
                parts.push(format!("\n--- FILE: {} (UNAVAILABLE) ---", src.file_path));
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
        parts.push(
            "\nAnswer based ONLY on the injected source code above. \
             Cite exact file paths and line numbers. \
             ONLY use the file paths listed above. DO NOT invent file names."
                .to_string(),
        );
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
        let ctx = CodeContext { symbols: vec![] };
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
        let ctx = CodeContext { symbols: vec![sym] };
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
        let ctx = CodeContext { symbols: vec![sym] };
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
            session_hint: None,
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
            session_hint: None,
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
            session_hint: None,
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
            session_hint: None,
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
            session_hint: None,
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
            session_hint: None,
        };
        let ctx = build_smart_context(input);
        assert!(ctx.contains("**Implementation**"));
        assert!(ctx.contains("**code explanation**"));
    }

    // ── v4.27.0 Body Enrichment tests ────────────────────────────────

    #[test]
    fn test_human_summary_trait_members() {
        let sym = serde_json::json!({
            "name": "SynapsePlugin",
            "kind": "Interface",
            "file_path": "crates/core/src/plugin.rs",
            "line_start": 23,
            "signature": "pub trait SynapsePlugin: Send + Sync {",
            "snippet": "pub trait SynapsePlugin: Send + Sync {\n    fn name(&self) -> &str;\n    fn on_init(&mut self, ctx: &SynapseContext) -> Result<()>;\n    fn on_event<'a>(&'a self, event: &'a SynapseEvent, ctx: &'a SynapseContext) -> Pin<Box<dyn Future<Output = Result<Option<SynapseEvent>>> + Send + 'a>>;\n    fn on_shutdown(&self, _ctx: &SynapseContext) -> Result<()> {\n        Ok(())\n    }\n    fn priority(&self) -> u32 {\n        100\n    }\n}"
        });
        let ctx = CodeContext { symbols: vec![sym] };
        let summary = build_human_summary(&Some(ctx)).unwrap();
        assert!(
            summary.contains("Methods:"),
            "should have Methods line: {summary}"
        );
        assert!(summary.contains("name"), "should list 'name': {summary}");
        assert!(
            summary.contains("on_init"),
            "should list 'on_init': {summary}"
        );
        assert!(
            summary.contains("on_event"),
            "should list 'on_event': {summary}"
        );
        assert!(
            summary.contains("on_shutdown"),
            "should list 'on_shutdown': {summary}"
        );
        assert!(
            summary.contains("priority"),
            "should list 'priority': {summary}"
        );
    }

    #[test]
    fn test_human_summary_struct_fields() {
        let sym = serde_json::json!({
            "name": "CodePatternScanner",
            "kind": "Struct",
            "file_path": "crates/husk/src/patterns.rs",
            "line_start": 47,
            "signature": "pub struct CodePatternScanner {",
            "snippet": "pub struct CodePatternScanner {\n    sql_patterns: Vec<Regex>,\n    xss_patterns: Vec<Regex>,\n    cmd_patterns: Vec<Regex>,\n    path_patterns: Vec<Regex>,\n    prompt_injection_patterns: Vec<Regex>,\n}"
        });
        let ctx = CodeContext { symbols: vec![sym] };
        let summary = build_human_summary(&Some(ctx)).unwrap();
        assert!(
            summary.contains("Fields:"),
            "should have Fields line: {summary}"
        );
        assert!(
            summary.contains("sql_patterns"),
            "should list 'sql_patterns': {summary}"
        );
        assert!(
            summary.contains("xss_patterns"),
            "should list 'xss_patterns': {summary}"
        );
        assert!(
            summary.contains("prompt_injection_patterns"),
            "should list 'prompt_injection_patterns': {summary}"
        );
    }

    #[test]
    fn test_human_summary_function_body_preview() {
        let sym = serde_json::json!({
            "name": "handle_tool_call",
            "kind": "Function",
            "file_path": "crates/mcp/src/tools/mod.rs",
            "line_start": 761,
            "signature": "pub fn handle_tool_call(",
            "snippet": "pub fn handle_tool_call(\n    name: &str,\n    args: &serde_json::Value,\n    ctx: &SynapseContext,\n) -> ToolCallResult {\n    if let Some(canonical) = resolve_tool_name(name) {\n        return dispatch_tool(canonical, args, ctx);\n    }\n    let mut best: Option<(&str, usize)> = None;\n    for &tool in TOOL_NAMES {\n        let dist = levenshtein(name, tool);\n    }\n}"
        });
        let ctx = CodeContext { symbols: vec![sym] };
        let summary = build_human_summary(&Some(ctx)).unwrap();
        assert!(
            summary.contains("Body:"),
            "should have Body line: {summary}"
        );
        assert!(
            summary.contains("resolve_tool_name"),
            "body should contain 'resolve_tool_name': {summary}"
        );
        assert!(
            summary.contains("TOOL_NAMES"),
            "body should contain 'TOOL_NAMES': {summary}"
        );
        assert!(
            summary.contains("levenshtein"),
            "body should contain 'levenshtein': {summary}"
        );
    }

    #[test]
    fn test_human_summary_no_enrichment_without_snippet() {
        // Existing symbols without "snippet" field should not get enrichment
        let sym = serde_json::json!({
            "name": "ask",
            "kind": "function",
            "file_path": "crates/whisper/src/router/mod.rs",
            "line_start": 136
        });
        let ctx = CodeContext { symbols: vec![sym] };
        let summary = build_human_summary(&Some(ctx)).unwrap();
        assert!(
            !summary.contains("Body:"),
            "should NOT have Body without snippet: {summary}"
        );
        assert!(
            !summary.contains("Methods:"),
            "should NOT have Methods without snippet: {summary}"
        );
        assert!(
            !summary.contains("Fields:"),
            "should NOT have Fields without snippet: {summary}"
        );
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
            session_hint: None,
        };
        let ctx = build_smart_context(input);
        assert!(ctx.contains("Phase: Stabilization"));
    }
}
