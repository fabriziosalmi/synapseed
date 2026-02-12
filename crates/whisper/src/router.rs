//! Intent Router — the Whisperer's brain.
//!
//! Classifies a natural-language query into an intent, extracts target
//! entities (files, symbols), then executes the appropriate subsystems
//! directly via Rust APIs (zero JSON-RPC overhead) and aggregates results.
//!
//! Level 0: Deterministic keyword heuristics.
//! Level 1 (future): Pluggable small-LLM classifier.

use std::path::Path;

use serde::Serialize;
use tracing::{debug, info};

use synapseed_core::context::SynapseContext;
use synapseed_cortex::graph::CodeGraph;
use synapseed_husk::guard::SecurityGuard;

use synapseed_chronos::historian::Historian;
use synapseed_search::indexer::SemanticIndex;
use synapseed_shadow_check::diagnostic::DiagnosticLevel;
use synapseed_shadow_check::runner::DiagnosticStore;

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
pub fn ask(query: &str, ctx: &SynapseContext) -> WhisperResult {
    info!(query = query, "Whisperer: Processing query");

    let intent = classify_intent(query);
    let complexity = analyze_complexity(query);
    debug!(intent = ?intent, complexity = ?complexity, "Whisperer: Classified");

    let targets = extract_targets(query, ctx);
    debug!(target_count = targets.len(), "Whisperer: Extracted targets");

    // Execute plan based on intent — each gather fn knows when to activate
    let diagnostics = gather_diagnostics(&intent, &targets, ctx);
    let history = gather_history(&intent, &targets, ctx);
    let code_context = gather_code_context(&intent, &targets, ctx);
    let security_status = gather_security(&intent, &targets, ctx);

    let smart_context = build_smart_context(
        query,
        &intent,
        complexity,
        &diagnostics,
        &history,
        &code_context,
        &security_status,
    );

    WhisperResult {
        intent,
        complexity,
        query: query.to_string(),
        targets,
        diagnostics,
        history,
        code_context,
        security_status,
        smart_context,
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
    targets.truncate(5);
    targets
}

// ── Plan Execution ─────────────────────────────────────────────────────

fn gather_diagnostics(
    intent: &Intent,
    targets: &[Target],
    ctx: &SynapseContext,
) -> Option<DiagnosticsContext> {
    if !matches!(intent, Intent::BugFix | Intent::Refactor | Intent::General) {
        return None;
    }

    let store = ctx.get_extension::<DiagnosticStore>()?;

    let file_paths: Vec<&str> = targets
        .iter()
        .filter_map(|t| t.file_path.as_deref())
        .collect();

    let diagnostics = if file_paths.is_empty() {
        store.snapshot().diagnostics
    } else {
        file_paths.iter().flat_map(|f| store.for_file(f)).collect()
    };

    let error_count = diagnostics
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .count();
    let warning_count = diagnostics
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Warning)
        .count();

    let items: Vec<serde_json::Value> = diagnostics
        .iter()
        .map(|d| serde_json::to_value(d).unwrap_or_default())
        .collect();

    Some(DiagnosticsContext {
        error_count,
        warning_count,
        items,
    })
}

fn gather_history(
    intent: &Intent,
    targets: &[Target],
    ctx: &SynapseContext,
) -> Option<HistoryContext> {
    if !matches!(
        intent,
        Intent::BugFix | Intent::Explain | Intent::Refactor | Intent::General
    ) {
        return None;
    }

    let historian = ctx.get_extension::<Historian>()?;

    // Analyze the first target file
    let target = targets.iter().find(|t| t.file_path.is_some())?;
    let file_path = target.file_path.as_deref()?;

    let analysis = historian.analyze_history(file_path, None, None).ok()?;

    let recent_commits: Vec<serde_json::Value> = analysis
        .commits
        .iter()
        .take(5)
        .map(|c| serde_json::to_value(c).unwrap_or_default())
        .collect();

    Some(HistoryContext {
        file: file_path.to_string(),
        total_commits: analysis.total_commits,
        hotspot_score: analysis.hotspot_score,
        risk: analysis.semantic_summary.risk_indicator.clone(),
        recent_commits,
        top_authors: analysis.top_authors.clone(),
    })
}

fn gather_code_context(
    intent: &Intent,
    targets: &[Target],
    ctx: &SynapseContext,
) -> Option<CodeContext> {
    if !matches!(
        intent,
        Intent::BugFix | Intent::Explain | Intent::Refactor | Intent::General
    ) {
        return None;
    }

    let root = ctx.project_root();
    let graph = CodeGraph::new();
    graph.index_directory(&root).ok()?;

    let mut symbols = Vec::new();
    for target in targets {
        for sym in graph.lookup(&target.name).into_iter().take(3) {
            symbols.push(serde_json::to_value(&sym).unwrap_or_default());
        }
    }

    if symbols.is_empty() {
        return None;
    }

    // Dedup by symbol name
    symbols.dedup_by(|a, b| a["name"] == b["name"]);
    Some(CodeContext { symbols })
}

fn gather_security(intent: &Intent, targets: &[Target], ctx: &SynapseContext) -> String {
    // Always scan for Security intent; also scan for BugFix (might reveal root cause)
    if !matches!(intent, Intent::Security | Intent::BugFix) {
        return "NOT_SCANNED".to_string();
    }

    let root = ctx.project_root();
    let guard = SecurityGuard::with_defaults();

    // If we have specific target files, scan them
    let mut findings = Vec::new();
    for target in targets {
        if let Some(file_path) = &target.file_path {
            let abs_path = if Path::new(file_path).is_absolute() {
                file_path.into()
            } else {
                root.join(file_path)
            };

            if let Ok(content) = std::fs::read_to_string(&abs_path) {
                if let Err(e) = guard.check(&content) {
                    findings.push(format!("{}: {}", file_path, e));
                }
            }
        }
    }

    // If no targets but Security intent, scan all indexed source files
    if findings.is_empty() && matches!(intent, Intent::Security) && targets.is_empty() {
        let graph = CodeGraph::new();
        if graph.index_directory(&root).is_ok() {
            for file in graph.all_files() {
                let abs_path = root.join(&file.path);
                if let Ok(content) = std::fs::read_to_string(&abs_path) {
                    if let Err(e) = guard.check(&content) {
                        findings.push(format!("{}: {}", file.path, e));
                    }
                }
            }
        }
    }

    if findings.is_empty() {
        "CLEAN".to_string()
    } else {
        format!("ALERT: {}", findings.join("; "))
    }
}

// ── Smart Context Builder ──────────────────────────────────────────────

fn build_smart_context(
    query: &str,
    intent: &Intent,
    complexity: QueryComplexity,
    diagnostics: &Option<DiagnosticsContext>,
    history: &Option<HistoryContext>,
    code_context: &Option<CodeContext>,
    security_status: &str,
) -> String {
    let intent_label = match intent {
        Intent::BugFix => "bug fix",
        Intent::Security => "security audit",
        Intent::Explain => "code explanation",
        Intent::Refactor => "refactoring",
        Intent::General => "general inquiry",
    };

    // HCI Req 5 (Mentor Mode): adapt preamble to query complexity
    let preamble = match complexity {
        QueryComplexity::Quick => format!(
            "Brief answer for \"{query}\" ({intent_label}):"
        ),
        QueryComplexity::Standard => format!(
            "Based on your query \"{query}\", SYNAPSEED detected a **{intent_label}** intent and gathered:"
        ),
        QueryComplexity::Deep => format!(
            "Detailed analysis for \"{query}\" — detected intent: **{intent_label}**.\n\
             SYNAPSEED gathered comprehensive context across all subsystems:"
        ),
    };

    let mut parts = vec![preamble];
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
            parts.push(format!(
                "- **History** ({}): {} commit(s), hotspot {:.1}, risk: {}",
                hist.file, hist.total_commits, hist.hotspot_score, hist.risk
            ));
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

    let closing = match complexity {
        QueryComplexity::Quick => "\nProvide a concise answer.",
        QueryComplexity::Standard => "\nUse the full JSON context below to provide an informed, precise answer.",
        QueryComplexity::Deep => "\nUse ALL gathered context to provide a thorough, cross-referenced analysis with specific file paths and line numbers.",
    };
    parts.push(closing.into());
    parts.join("\n")
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
}
