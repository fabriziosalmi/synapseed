//! Intent Router — the Whisperer's brain.
//!
//! Classifies a natural-language query into an intent, extracts target
//! entities (files, symbols), then executes the appropriate subsystems
//! directly via Rust APIs (zero JSON-RPC overhead) and aggregates results.
//!
//! Level 0: Deterministic keyword heuristics.
//! Level 1 (future): Pluggable small-LLM classifier.

mod code;
mod diagnostics;
mod history;
mod security;

use parking_lot::Mutex;
use serde::Serialize;
use tracing::{debug, info};

use synapseed_core::context::SynapseContext;
use synapseed_core::momentum::{ModelTier, MomentumEngine, SessionPhase};
use synapseed_cortex::graph::CodeGraph;
use synapseed_search::indexer::SemanticIndex;

// ── Types ──────────────────────────────────────────────────────────────

/// Detected intent category.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Intent {
    BugFix,
    Security,
    Explain,
    Refactor,
    General,
}

/// A target entity extracted from the query.
#[derive(Debug, Clone, Serialize)]
pub struct Target {
    pub kind: TargetKind,
    pub name: String,
    pub file_path: Option<String>,
    pub line_start: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    File,
    Symbol,
}

/// Compiler diagnostics gathered for the query.
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticsContext {
    pub error_count: usize,
    pub warning_count: usize,
    pub items: Vec<serde_json::Value>,
}

/// Git history gathered for the query.
#[derive(Debug, Clone, Serialize)]
pub struct HistoryContext {
    pub file: String,
    pub total_commits: usize,
    pub hotspot_score: f64,
    pub risk: String,
    pub recent_commits: Vec<serde_json::Value>,
    pub top_authors: Vec<(String, usize)>,
    pub convergence_rate: f64,
    pub rigidity: f64,
    pub fix_chain_count: usize,
}

/// Code structure gathered for the query.
#[derive(Debug, Clone, Serialize)]
pub struct CodeContext {
    pub symbols: Vec<serde_json::Value>,
}

/// The full aggregated result from the Whisperer.
#[derive(Debug, Clone, Serialize)]
pub struct WhisperResult {
    pub intent: Intent,
    pub complexity: QueryComplexity,
    pub query: String,
    pub targets: Vec<Target>,
    pub diagnostics: Option<DiagnosticsContext>,
    pub history: Option<HistoryContext>,
    pub code_context: Option<CodeContext>,
    pub security_status: String,
    pub smart_context: String,
    /// Semantic Information Density: symbols_found / (prompt_tokens / 1000).
    /// Higher = more useful signal per token budget.
    pub sid: f64,
}

// ── Query Complexity (HCI Req 5: Mentor Mode) ─────────────────────────

/// How deep the Whisperer should go when building context.
/// Determined by simple string heuristics — no NLP, no external deps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryComplexity {
    /// Short/simple query → brief response, max 3 context sections
    Quick,
    /// Normal query → standard behavior
    Standard,
    /// Long/multi-part query → full context, cross-references
    Deep,
}

/// Classify query complexity from string heuristics.
/// Word count + question marks + code references → Quick/Standard/Deep.
pub fn analyze_complexity(query: &str) -> QueryComplexity {
    let word_count = query.split_whitespace().count();
    let question_marks = query.matches('?').count();
    let has_code_refs = query.contains("::")
        || query.contains("()")
        || query.contains(".rs")
        || query.contains(".py")
        || query.contains(".js");

    if word_count <= 4 && question_marks <= 1 && !has_code_refs {
        QueryComplexity::Quick
    } else if word_count >= 30 || question_marks > 1 || (word_count > 15 && has_code_refs) {
        QueryComplexity::Deep
    } else {
        QueryComplexity::Standard
    }
}

// ── Public API ─────────────────────────────────────────────────────────

/// Main entry point: analyze the query and return aggregated context.
///
/// Classifies intent, extracts targets, executes the right subsystems,
/// and returns everything the LLM needs in a single call.
///
/// HCI Req 5 (Mentor Mode): Response depth adapts to query complexity.
///
/// When `raw_injection` is true (v3.4.0+), the Whisperer reads the actual
/// source code for each discovered symbol and injects it verbatim into the
/// prompt, giving even sub-3B models enough context to answer accurately.
pub fn ask(query: &str, ctx: &SynapseContext) -> WhisperResult {
    ask_with_options(query, ctx, false)
}

/// Like [`ask`] but with explicit control over raw source injection.
pub fn ask_raw(query: &str, ctx: &SynapseContext, raw_injection: bool) -> WhisperResult {
    ask_with_options(query, ctx, raw_injection)
}

fn ask_with_options(query: &str, ctx: &SynapseContext, raw_injection: bool) -> WhisperResult {
    info!(query = query, raw = raw_injection, "Whisperer: Processing query");

    // ── Momentum: read tier + phase, check git staged (#52, #53, #54) ──
    let (tier, phase) = if let Some(engine) = ctx.get_extension::<Mutex<MomentumEngine>>() {
        let mut e = engine.lock();
        // Git-Context Alignment (#54): check for staged files
        let has_staged = detect_git_staged(ctx);
        e.set_git_staged(has_staged);
        (e.tier(), e.phase())
    } else {
        (ModelTier::default(), SessionPhase::default())
    };
    debug!(tier = %tier, phase = %phase, "Whisperer: Momentum state");

    let intent = classify_intent(query);
    let complexity = analyze_complexity(query);
    debug!(intent = ?intent, complexity = ?complexity, "Whisperer: Classified");

    let targets = extract_targets(query, ctx);
    debug!(target_count = targets.len(), "Whisperer: Extracted targets");

    // Execute plan based on intent — each gather fn knows when to activate
    let diag = diagnostics::gather_diagnostics(&intent, &targets, ctx);
    let hist = history::gather_history(&intent, &targets, ctx);
    let code_ctx = code::gather_code_context(&intent, &targets, ctx);
    let sec_status = security::gather_security(&intent, &targets, ctx);

    // ── Raw Source Injection (v3.4.0) ──────────────────────────────
    let raw_sources = if raw_injection {
        inject_raw_sources(&targets, ctx)
    } else {
        Vec::new()
    };

    let input = SmartContextInput {
        query,
        intent: &intent,
        complexity,
        diagnostics: &diag,
        history: &hist,
        code_context: &code_ctx,
        security_status: &sec_status,
        raw_injection,
        raw_sources: &raw_sources,
        tier,
        phase,
    };

    let smart_context = build_smart_context(input);

    // ── SID: Semantic Information Density ───────────────────────────
    // Formula: symbols_found / (prompt_tokens / 1000)
    // prompt_tokens ≈ smart_context.len() / 4 (rough char→token ratio)
    let symbols_found = code_ctx.as_ref().map_or(0, |c| c.symbols.len());
    let prompt_tokens = (smart_context.len() as f64 / 4.0).max(1.0);
    let sid = symbols_found as f64 / (prompt_tokens / 1000.0);

    WhisperResult {
        intent,
        complexity,
        query: query.to_string(),
        targets,
        diagnostics: diag,
        history: hist,
        code_context: code_ctx,
        security_status: sec_status,
        smart_context,
        sid,
    }
}

// ── Intent Classification ──────────────────────────────────────────────

const BUG_KEYWORDS: &[&str] = &[
    "fix", "error", "broken", "bug", "crash", "fail", "wrong", "issue", "rott", "compile", "panic",
    "cannot", "errore", "rotto", "problema",
];

const SECURITY_KEYWORDS: &[&str] = &[
    "security",
    "audit",
    "secret",
    "password",
    "vuln",
    "leak",
    "token",
    "key",
    "cve",
    "xss",
    "injection",
    "sicurezza",
    "segreto",
];

const EXPLAIN_KEYWORDS: &[&str] = &[
    "explain",
    "what is",
    "how does",
    "why",
    "understand",
    "describe",
    "what does",
    "cos'è",
    "perché",
    "come funziona",
    "spiega",
];

const REFACTOR_KEYWORDS: &[&str] = &[
    "refactor",
    "clean",
    "improve",
    "optimize",
    "restructure",
    "simplify",
    "extract",
    "rename",
    "move",
    "migliora",
    "pulisci",
];

fn classify_intent(query: &str) -> Intent {
    let lower = query.to_lowercase();

    // Score each intent by keyword matches (first match wins for ties)
    let bug_score = BUG_KEYWORDS.iter().filter(|k| lower.contains(**k)).count();
    let sec_score = SECURITY_KEYWORDS
        .iter()
        .filter(|k| lower.contains(**k))
        .count();
    let exp_score = EXPLAIN_KEYWORDS
        .iter()
        .filter(|k| lower.contains(**k))
        .count();
    let ref_score = REFACTOR_KEYWORDS
        .iter()
        .filter(|k| lower.contains(**k))
        .count();

    let max = bug_score.max(sec_score).max(exp_score).max(ref_score);

    if max == 0 {
        return Intent::General;
    }

    // Highest score wins; on tie, order of priority: BugFix > Security > Explain > Refactor
    if bug_score == max {
        Intent::BugFix
    } else if sec_score == max {
        Intent::Security
    } else if exp_score == max {
        Intent::Explain
    } else {
        Intent::Refactor
    }
}

// ── Target Extraction ──────────────────────────────────────────────────

/// Words to ignore when searching for symbols.
const STOP_WORDS: &[&str] = &[
    "the", "is", "a", "an", "in", "on", "at", "to", "for", "of", "and", "or", "why", "how", "what",
    "fix", "broken", "error", "explain", "security", "audit", "this", "that", "my", "code", "file",
    "it", "perché", "come", "cosa", "dove", "il", "la", "un", "una",
];

fn extract_targets(query: &str, ctx: &SynapseContext) -> Vec<Target> {
    let mut targets = Vec::new();
    let words: Vec<&str> = query.split_whitespace().collect();

    // Pass 1: Explicit file references (contain extension)
    for word in &words {
        let clean =
            word.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '/' && c != '_');
        if clean.contains('.')
            && (clean.ends_with(".rs")
                || clean.ends_with(".py")
                || clean.ends_with(".js")
                || clean.ends_with(".ts")
                || clean.ends_with(".toml"))
        {
            targets.push(Target {
                kind: TargetKind::File,
                name: clean.to_string(),
                file_path: Some(clean.to_string()),
                line_start: None,
            });
        }
    }

    // Pass 2: Semantic search for relevant symbols
    if let Some(index) = ctx.get_extension::<SemanticIndex>() {
        let results = index.search(query, 3);
        for r in results {
            targets.push(Target {
                kind: TargetKind::Symbol,
                name: r.symbol.clone(),
                file_path: Some(r.file.clone()),
                line_start: Some(r.line_start as usize),
            });
        }
    }

    // Pass 3: Fallback — cortex lookup on significant words
    if targets.is_empty() {
        let root = ctx.project_root();
        let graph = CodeGraph::new();
        if graph.index_directory(&root).is_ok() {
            for word in &words {
                let clean = word
                    .trim_matches(|c: char| !c.is_alphanumeric() && c != '_')
                    .to_lowercase();
                if clean.len() >= 3 && !STOP_WORDS.contains(&clean.as_str()) {
                    for sym in graph.lookup(&clean).into_iter().take(2) {
                        targets.push(Target {
                            kind: TargetKind::Symbol,
                            name: sym.name.clone(),
                            file_path: Some(sym.file_path.clone()),
                            line_start: Some(sym.line_start),
                        });
                    }
                }
            }
        }
    }

    // Dedup by (name, file_path)
    targets.dedup_by(|a, b| a.name == b.name && a.file_path == b.file_path);
    // Configurable pruning (v3.6.2): respect DNA context.max_symbols
    let max_symbols = ctx.dna().context.max_symbols;
    targets.truncate(max_symbols);
    targets
}

// ── Human Summary Builder ─────────────────────────────────────────────

/// Build a one-line-per-symbol summary for small LLMs that struggle with
/// raw JSON.  Output example:
///
/// ```text
/// Found fn `ask(query: &str, ctx: &SynapseContext) -> WhisperResult` in crates/whisper/src/router/mod.rs at line 136
/// Found struct `WhisperResult` in crates/whisper/src/router/mod.rs at line 81
/// ```
fn build_human_summary(code_context: &Option<CodeContext>) -> Option<String> {
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

/// A raw source code snippet extracted from disk for a discovered symbol.
#[derive(Debug, Clone, Serialize)]
pub struct RawSource {
    pub file_path: String,
    pub line_start: usize,
    pub line_end: usize,
    pub source: String,
}

/// Read the actual source code for each target that has file/line info.
fn inject_raw_sources(targets: &[Target], ctx: &SynapseContext) -> Vec<RawSource> {
    const CHAR_BUDGET: usize = 16_000; // ~4 000 tokens at 4 chars/token
    let root = ctx.project_root();
    let mut sources = Vec::new();
    let mut budget_used: usize = 0;

    // Retrieve the code graph from the context for precise line range lookup
    let graph = ctx.get_extension::<CodeGraph>();

    for target in targets {
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
        let e = end.max(s).min(lines.len());

        let snippet: String = lines[(s - 1)..e].join("\n");

        // Enforce token budget — stop injecting once we exceed ~4 000 tokens
        if budget_used + snippet.len() > CHAR_BUDGET {
            break;
        }
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

/// Input context for the smart context builder.
struct SmartContextInput<'a> {
    query: &'a str,
    intent: &'a Intent,
    complexity: QueryComplexity,
    diagnostics: &'a Option<DiagnosticsContext>,
    history: &'a Option<HistoryContext>,
    code_context: &'a Option<CodeContext>,
    security_status: &'a str,
    raw_injection: bool,
    raw_sources: &'a [RawSource],
    tier: ModelTier,
    phase: SessionPhase,
}

// ── Smart Context Builder ──────────────────────────────────────────────

fn build_smart_context(input: SmartContextInput) -> String {
    let query = input.query;
    let intent = input.intent;
    let complexity = input.complexity;
    let diagnostics = input.diagnostics;
    let history = input.history;
    let code_context = input.code_context;
    let security_status = input.security_status;
    let raw_injection = input.raw_injection;
    let raw_sources = input.raw_sources;
    let tier = input.tier;
    let phase = input.phase;

    let intent_label = match intent {
        Intent::BugFix => "bug fix",
        Intent::Security => "security audit",
        Intent::Explain => "code explanation",
        Intent::Refactor => "refactoring",
        Intent::General => "general inquiry",
    };

    // ── Tier-Adapted Preamble (#51) ─────────────────────────────────
    let preamble = match tier {
        ModelTier::Atomic => {
            // Flat, minimal — no markdown formatting for tiny models
            format!("{query} — {intent_label}")
        }
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

    // ── Atomic Tier: minimal output, skip structured sections ──────
    if tier == ModelTier::Atomic {
        // Raw injection still works for Atomic (that's the whole point)
        if raw_injection && !raw_sources.is_empty() {
            parts.push(String::new());
            parts.push("Source code:".into());
            for src in raw_sources {
                if src.line_start == 0 && src.line_end == 0 {
                    parts.push(format!("File: {} (UNAVAILABLE)", src.file_path));
                    parts.push(src.source.clone());
                } else {
                    parts.push(format!(
                        "File: {} lines {}-{}",
                        src.file_path, src.line_start, src.line_end
                    ));
                    parts.push(src.source.clone());
                }
            }
            parts.push(
                "\nAnswer using only the source code above. Cite file paths and line numbers.".into(),
            );
        } else {
            parts.push("\nProvide a concise answer.".into());
        }
        return parts.join("\n");
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
            } else {
                parts.push("- **Compiler**: No errors or warnings".into());
            }
            section_count += 1;
        }
    }

    if section_count < max_sections {
        if let Some(hist) = history {
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
            section_count += 1;
        }
    }

    if section_count < max_sections {
        if let Some(code) = code_context {
            parts.push(format!(
                "- **Code**: {} relevant symbol(s) found",
                code.symbols.len()
            ));
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

    let closing = if raw_injection {
        "\nAnswer based ONLY on the injected source code above. \
         Cite exact file paths and line numbers. \
         ONLY use the file paths listed above. DO NOT invent file names."
    } else {
        match complexity {
            QueryComplexity::Quick => "\nProvide a concise answer.",
            QueryComplexity::Standard => "\nUse the full JSON context below to provide an informed, precise answer.",
            QueryComplexity::Deep => "\nUse ALL gathered context to provide a thorough, cross-referenced analysis with specific file paths and line numbers.",
        }
    };
    parts.push(closing.into());
    parts.join("\n")
}

// ── Git Staged Detection (#54) ──────────────────────────────────────────

/// Detect whether git has staged files in the working directory.
/// Sub-ms: spawns `git diff --cached --name-only` synchronously.
fn detect_git_staged(ctx: &SynapseContext) -> bool {
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

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_bug_fix() {
        assert!(matches!(
            classify_intent("fix the broken login"),
            Intent::BugFix
        ));
        assert!(matches!(
            classify_intent("why is this error happening"),
            Intent::BugFix
        ));
        assert!(matches!(
            classify_intent("the code fails to compile"),
            Intent::BugFix
        ));
    }

    #[test]
    fn test_classify_security() {
        assert!(matches!(
            classify_intent("run a security audit"),
            Intent::Security
        ));
        assert!(matches!(
            classify_intent("check for leaked secrets"),
            Intent::Security
        ));
        assert!(matches!(
            classify_intent("is there a password in the code"),
            Intent::Security
        ));
    }

    #[test]
    fn test_classify_explain() {
        assert!(matches!(
            classify_intent("explain the authentication flow"),
            Intent::Explain
        ));
        assert!(matches!(
            classify_intent("what is SynapseContext"),
            Intent::Explain
        ));
        assert!(matches!(
            classify_intent("how does the router work"),
            Intent::Explain
        ));
    }

    #[test]
    fn test_classify_refactor() {
        assert!(matches!(
            classify_intent("refactor the parser module"),
            Intent::Refactor
        ));
        assert!(matches!(
            classify_intent("optimize the search index"),
            Intent::Refactor
        ));
    }

    #[test]
    fn test_classify_general() {
        assert!(matches!(
            classify_intent("list all modules"),
            Intent::General
        ));
        assert!(matches!(
            classify_intent("show me the project structure"),
            Intent::General
        ));
    }

    #[test]
    fn test_classify_priority_on_tie() {
        // "fix" (BugFix) + "security" (Security) → BugFix wins on priority
        assert!(matches!(
            classify_intent("fix the security issue"),
            Intent::BugFix
        ));
    }

    #[test]
    fn test_complexity_quick() {
        assert_eq!(analyze_complexity("what is this"), QueryComplexity::Quick);
        assert_eq!(analyze_complexity("help"), QueryComplexity::Quick);
        assert_eq!(analyze_complexity("fix it"), QueryComplexity::Quick);
    }

    #[test]
    fn test_complexity_standard() {
        assert_eq!(
            analyze_complexity("explain how the router works"),
            QueryComplexity::Standard
        );
        assert_eq!(
            analyze_complexity("what does the authentication module do"),
            QueryComplexity::Standard
        );
    }

    #[test]
    fn test_complexity_deep() {
        let long = "I need to understand how the authentication flow works across the entire codebase, including session management, token validation, and the security guard module. Can you also check for any vulnerabilities?";
        assert_eq!(analyze_complexity(long), QueryComplexity::Deep);
        // Multiple question marks
        assert_eq!(
            analyze_complexity("what is this? why does it fail? how to fix?"),
            QueryComplexity::Deep
        );
    }

    #[test]
    fn test_classify_italian() {
        assert!(matches!(
            classify_intent("perché la login è rotta"),
            Intent::BugFix
        ));
        assert!(matches!(
            classify_intent("spiega come funziona il router"),
            Intent::Explain
        ));
    }

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
            history: &None,
            code_context: &code_ctx,
            security_status: "CLEAN",
            raw_injection: false,
            raw_sources: &[],
            tier: ModelTier::Galactic,
            phase: SessionPhase::Discovery,
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
            history: &None,
            code_context: &None,
            security_status: "CLEAN",
            raw_injection: true,
            raw_sources: &raw_sources,
            tier: ModelTier::Galactic,
            phase: SessionPhase::Discovery,
        };

        let ctx = build_smart_context(input);
        assert!(ctx.contains("## Injected Source Code"));
        assert!(ctx.contains("--- FILE: src/main.rs (lines 1-2) ---"));
        assert!(ctx.contains("fn main()"));
        assert!(ctx.contains("Answer based ONLY on the injected source code"));
        assert!(ctx.contains("DO NOT invent file names"));
    }

    // ── Tier-adapted output tests (#51) ─────────────────────────────

    #[test]
    fn test_atomic_tier_flat_output() {
        let input = SmartContextInput {
            query: "explain main",
            intent: &Intent::Explain,
            complexity: QueryComplexity::Standard,
            diagnostics: &None,
            history: &None,
            code_context: &None,
            security_status: "CLEAN",
            raw_injection: false,
            raw_sources: &[],
            tier: ModelTier::Atomic,
            phase: SessionPhase::Discovery,
        };
        let ctx = build_smart_context(input);
        // Atomic: no markdown formatting, no ** bold
        assert!(!ctx.contains("**"));
        assert!(ctx.contains("explain main"));
        assert!(ctx.contains("concise answer"));
    }

    #[test]
    fn test_atomic_tier_raw_injection() {
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
            history: &None,
            code_context: &None,
            security_status: "CLEAN",
            raw_injection: true,
            raw_sources: &raw_sources,
            tier: ModelTier::Atomic,
            phase: SessionPhase::Discovery,
        };
        let ctx = build_smart_context(input);
        // Atomic raw: simplified delimiters (no ##, no ---)
        assert!(ctx.contains("Source code:"));
        assert!(ctx.contains("File: src/lib.rs lines 1-5"));
        assert!(ctx.contains("pub fn hello()"));
        // No "## Injected Source Code" header
        assert!(!ctx.contains("## Injected Source Code"));
    }

    #[test]
    fn test_galactic_tier_includes_phase() {
        let input = SmartContextInput {
            query: "explain the router architecture in detail with cross references",
            intent: &Intent::Explain,
            complexity: QueryComplexity::Standard,
            diagnostics: &None,
            history: &None,
            code_context: &None,
            security_status: "CLEAN",
            raw_injection: false,
            raw_sources: &[],
            tier: ModelTier::Galactic,
            phase: SessionPhase::Implementation,
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
            history: &None,
            code_context: &None,
            security_status: "CLEAN",
            raw_injection: false,
            raw_sources: &[],
            tier: ModelTier::Molecular,
            phase: SessionPhase::Stabilization,
        };
        let ctx = build_smart_context(input);
        assert!(ctx.contains("Phase: Stabilization"));
    }
}
