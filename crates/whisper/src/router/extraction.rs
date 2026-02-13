//! Target extraction pipeline — finds files and symbols relevant to a query.
//!
//! 5-pass extraction:
//! 1. Explicit file references (contain extension)
//! 2. Semantic search via Tantivy BM25 (with stop-word cleaning + synonym expansion)
//! 3. Cortex fallback for unmatched queries
//! 4. Implementation Twin — derive source paths from test paths
//! 5. Call Graph Lite — extract identifiers from test bodies

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use tracing::debug;

use synapseed_core::context::SynapseContext;
use synapseed_core::symbol::SymbolKind;
use synapseed_cortex::graph::CodeGraph;
use synapseed_search::indexer::SemanticIndex;

use super::{Target, TargetKind};

// ── Stop Words ─────────────────────────────────────────────────────────

/// Words to ignore when searching for symbols and when cleaning queries.
/// Using HashSet for O(1) lookup instead of O(n) array scan.
pub(super) static STOP_WORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
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
        "viene", "gestita", "gestito", "gestire", "gestione", "chiamano", "chiamata",
        "chiamate", "chiama", "riga", "righe", "linea", "linee", "cartella", "progetto",
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
    ].into_iter().collect()
});

// ── Query Cleaning ─────────────────────────────────────────────────────

/// Strip stop words and return cleaned technical terms for Tantivy search.
/// "Come funziona il chunked transfer encoding in requests?" → "chunked transfer encoding requests"
pub(super) fn clean_query_for_search(query: &str) -> String {
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
pub(super) fn expand_synonyms(term: &str) -> Vec<String> {
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

// ── Target Extraction ──────────────────────────────────────────────────

pub(super) fn extract_targets(query: &str, ctx: &SynapseContext) -> Vec<Target> {
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
        .any(|t| t.file_path.as_deref().is_some_and(is_test_path));

    if has_test_targets {
        let graph = ctx.get_extension::<CodeGraph>();

        // Pass 4: Implementation Twin — derive source paths from test paths
        let test_targets: Vec<Target> = targets
            .iter()
            .filter(|t| t.file_path.as_deref().is_some_and(is_test_path))
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
pub(super) fn is_test_path(path: &str) -> bool {
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
pub(super) fn is_vendor_path(path: &str) -> bool {
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
pub(super) fn derive_source_paths(test_path: &str) -> Vec<String> {
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

    // Guard: empty base (e.g., file named "test_.py") → no useful candidates
    if base.is_empty() || base == "test" {
        return Vec::new();
    }

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
    static CALL_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(\w+)\.(\w+)\s*\(").expect("valid regex"));
    for cap in CALL_RE.captures_iter(&body_text) {
        identifiers.push(cap[2].to_string());
    }

    // Extract bare `function(` patterns (≥3 chars, not a stop word)
    static BARE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?:^|[^.\w])(\w{3,})\s*\(").expect("valid regex"));
    for cap in BARE_RE.captures_iter(&body_text) {
        let name = &cap[1];
        if !STOP_WORDS.contains(&name.to_lowercase().as_str()) && name != fn_name {
            identifiers.push(name.to_string());
        }
    }

    identifiers.sort();
    identifiers.dedup();
    identifiers
}

#[cfg(test)]
mod tests {
    use super::*;

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
