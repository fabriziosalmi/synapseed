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
use synapseed_core::symbol::SymbolKind;
use synapseed_cortex::graph::CodeGraph;
use synapseed_search::indexer::SemanticIndex;

// ── Types ──────────────────────────────────────────────────────────────

/// Detected intent category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<DiagnosticsContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<HistoryContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_context: Option<CodeContext>,
    pub security_status: String,
    pub smart_context: String,
    /// Semantic Information Density: symbols_found / (prompt_tokens / 1000).
    /// Higher = more useful signal per token budget.
    pub sid: f64,
    /// Raw source code snippets injected for discovered symbols.
    /// Exposed in JSON so external tools can consume code directly.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub raw_sources: Vec<RawSource>,
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

    // ── Semantic Ballast (v3.7.0): Atomic tier forces raw injection ──
    let effective_raw = raw_injection || tier == ModelTier::Atomic;

    let mut intent = classify_intent(query);
    let complexity = analyze_complexity(query);
    debug!(intent = ?intent, complexity = ?complexity, "Whisperer: Classified");

    let mut targets = extract_targets(query, ctx);

    // Atomic Greedy Pruning (v3.9.4): max 3 unique-file targets for sub-3B models.
    // Prefer diversity: one target per unique file path to maximize coverage.
    // Source-first ordering ensures implementation files come before test/vendor files.
    if tier == ModelTier::Atomic {
        // Drop vendor/static targets entirely — they waste precious Atomic slots
        let before = targets.len();
        targets.retain(|t| !t.file_path.as_deref().map_or(false, is_vendor_path));
        if targets.len() < before {
            debug!(before, after = targets.len(), "Whisper: dropped vendor/static targets");
        }
        if targets.len() > 3 {
            debug!(before = targets.len(), "Whisper: Atomic greedy pruning to 3 unique-file targets");
            let mut seen_files = std::collections::HashSet::new();
            targets.retain(|t| {
                let key = t.file_path.as_deref().unwrap_or("");
                seen_files.insert(key.to_string())
            });
            targets.truncate(3);
        }
    }
    debug!(target_count = targets.len(), "Whisperer: Extracted targets");

    // Intent Hardening: if query matched known symbols but intent is General,
    // promote to Explain — the user is asking about specific code entities.
    if intent == Intent::General
        && targets
            .iter()
            .any(|t| matches!(t.kind, TargetKind::Symbol))
    {
        debug!("Intent hardened: General -> Explain (query contains known symbols)");
        intent = Intent::Explain;
    }

    // Execute plan based on intent — each gather fn knows when to activate
    let diag = diagnostics::gather_diagnostics(&intent, &targets, ctx);
    let hist = history::gather_history(&intent, &targets, ctx);
    let code_ctx = code::gather_code_context(&intent, &targets, ctx);
    let sec_status = security::gather_security(&intent, &targets, ctx);

    // ── Raw Source Injection (v3.4.0) ──────────────────────────────
    // Atomic tier: always inject, with expanded budget (Semantic Ballast)
    let raw_sources = if effective_raw {
        inject_raw_sources(&targets, ctx, tier == ModelTier::Atomic)
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
        raw_injection: effective_raw,
        raw_sources: &raw_sources,
        tier,
        phase,
        project_root: ctx.project_root().display().to_string(),
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
        raw_sources,
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

/// Words to ignore when searching for symbols and when cleaning queries.
const STOP_WORDS: &[&str] = &[
    // English
    "the", "is", "a", "an", "in", "on", "at", "to", "for", "of", "and", "or", "why", "how", "what",
    "fix", "broken", "error", "explain", "security", "audit", "this", "that", "my", "code", "file",
    "it", "does", "do", "work", "works", "from", "with", "about", "are", "was", "were", "be",
    "been", "being", "have", "has", "had", "not", "but", "by", "can", "could", "would", "should",
    "all", "each", "every", "both", "few", "more", "most", "some", "such", "than", "too", "very",
    "just", "also", "into", "through", "between", "after", "before", "during", "where", "when",
    "which", "who", "whom", "whose", "there", "here", "then", "out", "up", "down",
    // Italian — articles, prepositions, pronouns
    "perché", "come", "cosa", "dove", "il", "la", "un", "una", "lo", "gli", "le", "dei", "del",
    "della", "delle", "degli", "nel", "nella", "nelle", "nei", "negli", "con", "per", "tra", "fra",
    "che", "chi", "cui", "quale", "quali", "questo", "questa", "questi", "queste", "quello",
    "quella", "quelli", "quelle", "suo", "sua", "suoi", "sue", "mio", "mia", "nostro", "nostra",
    "sono", "sei", "siamo", "siete", "hanno", "avere", "essere", "fare", "funziona", "spiega",
    "descrivi", "mostra", "dimmi",
    // Italian — verbs, nouns, adjectives commonly mixed with technical terms
    "viene", "viene", "gestita", "gestito", "gestire", "gestione", "chiamano", "chiamata",
    "chiamate", "chiama", "riga", "righe", "linea", "linee", "file", "cartella", "progetto",
    "funzione", "funzioni", "metodo", "metodi", "classe", "classi", "variabile", "variabili",
    "tipo", "tipi", "valore", "valori", "parametro", "parametri", "argomento", "argomenti",
    "risultato", "risultati", "errore", "errori", "problema", "problemi",
    "quando", "ogni", "altro", "altra", "altri", "altre", "primo", "secondo", "terzo",
    "nuovo", "nuova", "nuovi", "nuove", "stesso", "stessa", "stessi", "stesse",
    "dentro", "fuori", "sopra", "sotto", "prima", "dopo", "durante", "sempre", "mai",
    "anche", "ancora", "già", "solo", "molto", "poco", "troppo", "tutto", "tutti",
    "parte", "parti", "modo", "modi", "punto", "punti", "caso", "casi",
    // Italian — technical verbs that don't add search value
    "decodifica", "codifica", "elabora", "elaborazione", "gestisce", "implementa",
    "implementazione", "utilizza", "utilizzata", "usa", "usata", "usato",
    "esegue", "eseguita", "eseguito", "restituisce", "ritorna", "passa", "riceve",
];

/// Strip stop words and return cleaned technical terms for Tantivy search.
/// "Come funziona il chunked transfer encoding in requests?" → "chunked transfer encoding requests"
fn clean_query_for_search(query: &str) -> String {
    let terms: Vec<&str> = query
        .split(|c: char| c.is_whitespace() || c == '?' || c == '!' || c == ',')
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '_'))
        .filter(|w| w.len() >= 2 && !STOP_WORDS.contains(&w.to_lowercase().as_str()))
        .collect();

    // Synonym expansion (v3.9.4): add morphological variants to improve BM25 recall.
    // "chunked" → also match "chunk", "encoding" → "encode decode", etc.
    let mut expanded = terms.iter().map(|t| t.to_string()).collect::<Vec<_>>();
    for term in &terms {
        let lower = term.to_lowercase();
        for synonym in expand_synonyms(&lower) {
            if !expanded.iter().any(|e| e.to_lowercase() == synonym) {
                expanded.push(synonym);
            }
        }
    }

    expanded.join(" ")
}

/// Expand a technical term with morphological variants and domain synonyms.
/// Returns additional terms to append to the search query.
fn expand_synonyms(term: &str) -> Vec<String> {
    let mut synonyms = Vec::new();

    // Morphological: strip common suffixes to get root form
    if let Some(root) = term.strip_suffix("ed") {
        synonyms.push(root.to_string()); // "chunked" → "chunk"
        synonyms.push(format!("{root}ing")); // "chunked" → "chunking"
    } else if let Some(root) = term.strip_suffix("ing") {
        synonyms.push(root.to_string()); // "encoding" → "encod"
        synonyms.push(format!("{root}e")); // "encoding" → "encode"
    } else if let Some(root) = term.strip_suffix("tion") {
        synonyms.push(root.to_string()); // "authentication" → "authentica"
        synonyms.push(format!("{root}te")); // "authentication" → "authenticate"
    }

    // Domain-specific synonyms (bidirectional)
    static SYNONYM_PAIRS: &[(&str, &[&str])] = &[
        ("encode", &["decode", "codec", "encoding"]),
        ("decode", &["encode", "codec", "decoding"]),
        ("chunk", &["stream", "iter", "iterate"]),
        ("route", &["router", "routing", "dispatch"]),
        ("handle", &["handler", "middleware"]),
        ("auth", &["authenticate", "authorization", "login"]),
        ("request", &["response", "http"]),
        ("parse", &["parser", "parsing", "deserialize"]),
        ("serialize", &["deserialize", "marshal", "unmarshal"]),
        ("connect", &["connection", "socket", "transport"]),
        ("transfer", &["transport", "stream"]),
    ];

    for &(key, values) in SYNONYM_PAIRS {
        if term == key || term.starts_with(key) {
            for &v in values {
                if !synonyms.contains(&v.to_string()) {
                    synonyms.push(v.to_string());
                }
            }
        }
    }

    synonyms
}

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

    // Pass 2: Semantic search for relevant symbols (with confidence thresholding)
    // Clean query: strip EN/IT stop words so Tantivy BM25 focuses on technical terms.
    let search_query = clean_query_for_search(query);
    let min_confidence = ctx.dna().context.min_confidence;
    if !search_query.is_empty() {
        if let Some(index) = ctx.get_extension::<SemanticIndex>() {
            debug!(raw = query, cleaned = %search_query, "Whisper: cleaned query for search");
            let results = index.search(&search_query, 5);
            for r in results {
                if r.score < min_confidence {
                    debug!(
                        symbol = %r.symbol, score = r.score, threshold = min_confidence,
                        "Whisper: dropping low-confidence search result"
                    );
                    continue;
                }
                targets.push(Target {
                    kind: TargetKind::Symbol,
                    name: r.symbol.clone(),
                    file_path: Some(r.file.clone()),
                    line_start: Some(r.line_start as usize),
                });
            }
        }
    }

    // Pass 3: Fallback — cortex lookup on significant words
    // Reuse the existing CodeGraph from context (populated by cortex plugin)
    // instead of creating a new one and re-indexing from scratch.
    // Skip Import/Variable symbols — they're noise (e.g., `import requests`).
    if targets.is_empty() {
        let graph = ctx.get_extension::<CodeGraph>().unwrap_or_else(|| {
            // Last resort: create a fresh graph and index synchronously
            let g = CodeGraph::new();
            let _ = g.index_directory(&ctx.project_root());
            std::sync::Arc::new(g)
        });
        for word in &words {
            let clean = word
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '_')
                .to_lowercase();
            if clean.len() >= 3 && !STOP_WORDS.contains(&clean.as_str()) {
                for sym in graph.lookup(&clean).into_iter().take(4) {
                    // Filter out Import and Variable — they're noise
                    if matches!(sym.kind, SymbolKind::Import | SymbolKind::Variable) {
                        continue;
                    }
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

    // ── Source-First Heuristics (v3.9.3) ──────────────────────────────
    // When search results are dominated by test files, expand to find the
    // actual implementation code that those tests exercise.

    let has_test_targets = targets
        .iter()
        .any(|t| t.file_path.as_deref().map_or(false, is_test_path));

    if has_test_targets {
        let graph = ctx.get_extension::<CodeGraph>();

        // Pass 4: Implementation Twin — derive source paths from test paths
        let test_targets: Vec<Target> = targets
            .iter()
            .filter(|t| t.file_path.as_deref().map_or(false, is_test_path))
            .cloned()
            .collect();

        for target in &test_targets {
            let fp = match &target.file_path {
                Some(p) => p.clone(),
                None => continue,
            };
            for candidate in derive_source_paths(&fp) {
                if let Some(ref g) = graph {
                    let abs_candidate = ctx.project_root().join(&candidate);
                    if let Some(file_struct) = g.hoist(&abs_candidate) {
                        debug!(
                            test = %fp, twin = %candidate,
                            "Whisper: Twin Pattern found implementation file"
                        );
                        for sym in file_struct.symbols.iter().take(3) {
                            if matches!(sym.kind, SymbolKind::Import | SymbolKind::Variable) {
                                continue;
                            }
                            targets.push(Target {
                                kind: TargetKind::Symbol,
                                name: sym.name.clone(),
                                file_path: Some(sym.file_path.clone()),
                                line_start: Some(sym.line_start),
                            });
                        }
                        break; // Found the twin, stop searching candidates
                    }
                }
            }
        }

        // Pass 5: Call Graph Lite — extract identifiers from test bodies
        if let Some(ref g) = graph {
            for target in &test_targets {
                let fp = match &target.file_path {
                    Some(p) => p,
                    None => continue,
                };
                let abs_path = ctx.project_root().join(fp);
                if let Ok(content) = std::fs::read_to_string(&abs_path) {
                    let identifiers = extract_call_identifiers(&content, &target.name);
                    debug!(
                        test_fn = %target.name, ids = ?identifiers,
                        "Whisper: Call Graph Lite extracted identifiers"
                    );
                    for ident in identifiers {
                        for sym in g.lookup(&ident).into_iter().take(2) {
                            if matches!(sym.kind, SymbolKind::Import | SymbolKind::Variable) {
                                continue;
                            }
                            if is_test_path(&sym.file_path) {
                                continue;
                            }
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
    }

    // Source-first ordering: source > test > vendor/static (v3.9.4)
    targets.sort_by_key(|t| {
        let fp = t.file_path.as_deref().unwrap_or("");
        if is_vendor_path(fp) {
            2
        } else if is_test_path(fp) {
            1
        } else {
            0
        }
    });

    // Dedup by (name, file_path)
    targets.dedup_by(|a, b| a.name == b.name && a.file_path == b.file_path);
    // Configurable pruning (v3.6.2): respect DNA context.max_symbols
    let max_symbols = ctx.dna().context.max_symbols;
    targets.truncate(max_symbols);

    // Relativize all file paths to project root.
    // CodeGraph and SemanticIndex store absolute paths; downstream consumers
    // (raw_sources, smart_context, code_context) need relative paths to avoid
    // leaking local filesystem structure and confusing small LLMs.
    let root = ctx.project_root();
    for target in &mut targets {
        if let Some(ref mut fp) = target.file_path {
            if let Ok(rel) = std::path::Path::new(fp).strip_prefix(&root) {
                *fp = rel.display().to_string();
            }
        }
    }

    targets
}

// ── Source-First Helpers (v3.9.3) ─────────────────────────────────────

/// Returns true if the path looks like a test file.
fn is_test_path(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    p.contains("/test/")
        || p.contains("/tests/")
        || p.starts_with("test/")
        || p.starts_with("tests/")
        || p.contains("test_")
        || p.contains("_test.")
        || p.contains(".test.")
        || p.contains("/spec/")
        || p.starts_with("spec/")
        || p.contains("_spec.")
        || p.contains(".spec.")
}

/// Returns true if the path looks like vendored/static/generated code.
fn is_vendor_path(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    p.contains("/vendor/")
        || p.contains("/node_modules/")
        || p.contains("/static/")
        || p.contains("/dist/")
        || p.contains("/build/")
        || p.contains("/generated/")
        || p.contains(".min.")
        || p.contains("/third_party/")
        || p.contains("/third-party/")
        || p.contains("/extern/")
        || p.contains("/deps/")
}

/// Derive candidate source file paths from a test file path.
///
/// `"tests/test_requests.py"` → `["src/requests.py", "src/requests/__init__.py", ...]`
fn derive_source_paths(test_path: &str) -> Vec<String> {
    let path = std::path::Path::new(test_path);
    let file_name = path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("");
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    // Strip test_ prefix and _test / .test suffix
    let base = file_name
        .strip_prefix("test_")
        .unwrap_or(file_name);
    let base = base
        .strip_suffix(&format!("_test.{ext}"))
        .or_else(|| base.strip_suffix(&format!(".test.{ext}")))
        .unwrap_or(base)
        .trim_end_matches(&format!(".{ext}"));

    let mut candidates = Vec::new();
    // Python conventions
    candidates.push(format!("src/{base}.{ext}"));
    candidates.push(format!("src/{base}/{base}.{ext}"));
    candidates.push(format!("src/{base}/__init__.{ext}"));
    candidates.push(format!("{base}.{ext}"));
    candidates.push(format!("{base}/{base}.{ext}"));
    // Rust conventions
    candidates.push(format!("src/{base}/mod.rs"));
    candidates.push(format!("src/{base}.rs"));
    // JS/TS conventions
    candidates.push(format!("src/{base}/index.{ext}"));
    candidates
}

/// Extract function/method call identifiers from the body of a named function.
///
/// Regex-based: looks for `module.method(` and bare `function(` patterns
/// within the function body (delimited by indentation for Python, braces
/// for Rust/JS).
fn extract_call_identifiers(source: &str, fn_name: &str) -> Vec<String> {
    let mut identifiers = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut in_body = false;
    let mut body_indent = 0usize;
    let mut body_text = String::new();

    for line in &lines {
        if !in_body {
            // Match function definition containing fn_name
            if line.contains(&format!("def {fn_name}"))
                || line.contains(&format!("fn {fn_name}"))
                || line.contains(&format!("function {fn_name}"))
            {
                in_body = true;
                body_indent = line.len() - line.trim_start().len() + 4;
            }
        } else {
            let trimmed = line.trim_start();
            let indent = line.len() - trimmed.len();
            // End of function: non-empty line at same or lower indent (Python)
            if !trimmed.is_empty()
                && indent < body_indent
                && !trimmed.starts_with('#')
                && !trimmed.starts_with("//")
            {
                break;
            }
            body_text.push_str(line);
            body_text.push('\n');
        }
    }

    // Extract `module.method(` patterns → take the method name
    if let Ok(call_re) = regex::Regex::new(r"(\w+)\.(\w+)\s*\(") {
        for cap in call_re.captures_iter(&body_text) {
            identifiers.push(cap[2].to_string());
        }
    }

    // Extract bare `function(` patterns (≥3 chars, not a stop word)
    if let Ok(bare_re) = regex::Regex::new(r"(?:^|[^.\w])(\w{3,})\s*\(") {
        for cap in bare_re.captures_iter(&body_text) {
            let name = &cap[1];
            if !STOP_WORDS.contains(&name.to_lowercase().as_str()) && name != fn_name {
                identifiers.push(name.to_string());
            }
        }
    }

    identifiers.sort();
    identifiers.dedup();
    identifiers
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

/// When `atomic_mode` is true (Semantic Ballast), the budget is doubled and
/// each snippet is expanded to at least 30 lines to give small models enough
/// grounding context.
fn inject_raw_sources(targets: &[Target], ctx: &SynapseContext, atomic_mode: bool) -> Vec<RawSource> {
    let char_budget: usize = if atomic_mode { 32_000 } else { 16_000 };
    let min_lines: usize = if atomic_mode { 30 } else { 0 };
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
        let mut e = end.max(s).min(lines.len());

        // Semantic Ballast: ensure at least min_lines per snippet
        if min_lines > 0 && (e - s + 1) < min_lines {
            e = (s + min_lines - 1).min(lines.len());
        }

        let snippet: String = lines[(s - 1)..e].join("\n");
        let snippet = minify_source(&snippet);

        // Enforce token budget — stop injecting once we exceed the budget
        if budget_used + snippet.len() > char_budget {
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
    project_root: String,
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
        parts.push(format!(
            "\nAnswer based ONLY on the injected source code above. \
             Cite exact file paths and line numbers. \
             ONLY use the file paths listed above. DO NOT invent file names."
        ));
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
            history: &None,
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
            history: &None,
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
            history: &None,
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
            history: &None,
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
            history: &None,
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
            history: &None,
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

    // ── Query cleaning tests (v3.9.0) ─────────────────────────────

    #[test]
    fn test_clean_query_italian_stops_removed() {
        let cleaned = clean_query_for_search("Come funziona il chunked transfer encoding in requests?");
        // Core terms preserved
        assert!(cleaned.starts_with("chunked transfer encoding requests"));
        // Synonym expansion adds related terms
        assert!(cleaned.contains("chunk")); // from "chunked"
        assert!(cleaned.contains("stream")); // synonym of chunk
    }

    #[test]
    fn test_clean_query_italian_extended_stops() {
        // v3.9.4: "viene gestita decodifica funzioni chiamano riga" should all be removed
        let cleaned = clean_query_for_search(
            "In quale file e riga viene gestita la decodifica del chunked transfer encoding e quali funzioni lo chiamano?"
        );
        // Core technical terms preserved, synonyms expanded
        assert!(cleaned.contains("chunked"));
        assert!(cleaned.contains("transfer"));
        assert!(cleaned.contains("encoding"));
        // Synonym expansion: "chunked" → "chunk", "encoding" → "encode"
        assert!(cleaned.contains("chunk"));
        assert!(cleaned.contains("stream")); // synonym of chunk
        // Italian noise removed
        assert!(!cleaned.contains("viene"));
        assert!(!cleaned.contains("gestita"));
        assert!(!cleaned.contains("decodifica"));
        assert!(!cleaned.contains("riga"));
        assert!(!cleaned.contains("funzioni"));
        assert!(!cleaned.contains("chiamano"));
    }

    #[test]
    fn test_clean_query_english_stops_removed() {
        let cleaned = clean_query_for_search("How does the authentication flow work in this project?");
        // Core terms preserved (synonyms may be appended)
        assert!(cleaned.starts_with("authentication flow project"));
        // "authentication" → synonym expansion via "tion" suffix stripping + auth synonyms
        assert!(cleaned.contains("authenticate"));
    }

    #[test]
    fn test_clean_query_preserves_technical_terms() {
        let cleaned = clean_query_for_search("explain tokio::spawn and async runtime");
        assert!(cleaned.contains("tokio::spawn"));
        assert!(cleaned.contains("async"));
        assert!(cleaned.contains("runtime"));
    }

    #[test]
    fn test_synonym_expansion() {
        // "chunked" → "chunk", "chunking"
        let synonyms = expand_synonyms("chunked");
        assert!(synonyms.contains(&"chunk".to_string()));

        // "encoding" → "encode" via morphological strip
        let synonyms = expand_synonyms("encoding");
        assert!(synonyms.contains(&"encode".to_string()));

        // "chunk" → domain synonyms "stream", "iter"
        let synonyms = expand_synonyms("chunk");
        assert!(synonyms.contains(&"stream".to_string()));
        assert!(synonyms.contains(&"iter".to_string()));

        // Unknown word → no synonyms
        let synonyms = expand_synonyms("foobar");
        assert!(synonyms.is_empty());
    }

    #[test]
    fn test_clean_query_empty_result() {
        let cleaned = clean_query_for_search("come il la un");
        assert!(cleaned.is_empty());
    }

    // ── Source-First Heuristics tests (v3.9.3) ───────────────────────

    #[test]
    fn test_is_test_path_detects_test_directories() {
        assert!(is_test_path("tests/test_lowlevel.py"));
        assert!(is_test_path("src/tests/helper.rs"));
        assert!(is_test_path("/abs/path/tests/unit.py"));
        assert!(is_test_path("test/integration.js"));
        assert!(is_test_path("spec/models_spec.rb"));
    }

    #[test]
    fn test_is_test_path_detects_test_files() {
        assert!(is_test_path("test_requests.py"));
        assert!(is_test_path("models_test.go"));
        assert!(is_test_path("auth.test.ts"));
        assert!(is_test_path("handler_spec.rb"));
        assert!(is_test_path("utils.spec.js"));
    }

    #[test]
    fn test_is_test_path_rejects_source_files() {
        assert!(!is_test_path("src/requests/models.py"));
        assert!(!is_test_path("src/main.rs"));
        assert!(!is_test_path("lib/auth/handler.ts"));
        assert!(!is_test_path("crates/core/src/context.rs"));
        // Edge case: "test" in module name should not match
        assert!(!is_test_path("src/attestation/verify.rs"));
    }

    #[test]
    fn test_derive_source_paths_python() {
        let paths = derive_source_paths("tests/test_requests.py");
        assert!(paths.contains(&"src/requests.py".to_string()));
        assert!(paths.contains(&"src/requests/__init__.py".to_string()));
        assert!(paths.contains(&"src/requests/requests.py".to_string()));
        assert!(paths.contains(&"requests.py".to_string()));
    }

    #[test]
    fn test_derive_source_paths_rust() {
        let paths = derive_source_paths("tests/test_router.rs");
        assert!(paths.contains(&"src/router/mod.rs".to_string()));
        assert!(paths.contains(&"src/router.rs".to_string()));
    }

    #[test]
    fn test_derive_source_paths_js_suffix() {
        let paths = derive_source_paths("__tests__/auth.test.ts");
        // file_name = "auth.test.ts", strip .test.ts → base = "auth"
        assert!(paths.contains(&"src/auth.ts".to_string()));
        assert!(paths.contains(&"src/auth/index.ts".to_string()));
    }

    #[test]
    fn test_extract_call_identifiers_python() {
        let source = r#"
import requests
from requests import Response

def test_chunked_encoding_error():
    """get a ChunkedEncodingError if the server returns a bad response"""
    server = Server(incomplete_handler)
    with server as (host, port):
        url = f"http://{host}:{port}/"
        with pytest.raises(requests.exceptions.ChunkedEncodingError):
            requests.get(url)
        close_server.set()

def test_other():
    pass
"#;
        let ids = extract_call_identifiers(source, "test_chunked_encoding_error");
        assert!(ids.contains(&"Server".to_string()));
        assert!(ids.contains(&"get".to_string()));
        assert!(ids.contains(&"raises".to_string()));
        assert!(ids.contains(&"set".to_string()));
        // Should NOT include the function itself
        assert!(!ids.contains(&"test_chunked_encoding_error".to_string()));
    }

    #[test]
    fn test_extract_call_identifiers_rust() {
        let source = r#"
fn test_ask_returns_result() {
    let ctx = init_context();
    let result = whisper::ask("explain main", &ctx);
    assert!(result.targets.len() > 0);
}

fn other_fn() {}
"#;
        let ids = extract_call_identifiers(source, "test_ask_returns_result");
        assert!(ids.contains(&"init_context".to_string()));
        assert!(ids.contains(&"ask".to_string()));
        assert!(ids.contains(&"len".to_string()));
    }

    #[test]
    fn test_extract_call_identifiers_empty_body() {
        let source = "def test_empty():\n    pass\n";
        let ids = extract_call_identifiers(source, "test_empty");
        assert!(ids.is_empty());
    }

    #[test]
    fn test_extract_call_identifiers_missing_fn() {
        let source = "def unrelated():\n    foo()\n";
        let ids = extract_call_identifiers(source, "nonexistent");
        assert!(ids.is_empty());
    }
}
