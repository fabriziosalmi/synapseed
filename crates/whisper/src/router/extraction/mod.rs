//! Target extraction pipeline — finds files and symbols relevant to a query.
//!
//! 7-pass extraction:
//! 1. Explicit file references (contain extension)
//! 2. Hybrid RRF search: BM25 + vector fusion (v4.5.0), falls back to BM25-only
//! 3. Cortex fallback for unmatched queries
//! 4. Implementation Twin — derive source paths from test paths
//! 5. Call Graph Lite — extract identifiers from test bodies
//! 6. Python Config String Resolver (v4.2.0)
//! 7. Reverse Test Lookup (v4.6.0) — "Il Ponte della Verità"
//!
//! Split into submodules (#64):
//! - `query.rs` — stop words, query cleaning, CamelCase splitting, synonym expansion

mod query;

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use tracing::debug;

use synapseed_core::context::SynapseContext;
use synapseed_core::symbol::SymbolKind;
use synapseed_cortex::graph::CodeGraph;
// D40: Import trait for backend-agnostic search; concrete type still used for
// get_extension (TypeId-based), but all method calls go through the trait.
use synapseed_search::backend::SearchBackend;
use synapseed_search::indexer::SemanticIndex;

use super::{Target, TargetKind};
pub(super) use query::{clean_query_for_search, STOP_WORDS};

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
                score: None,
            });
        }
    }

    // Pass 2: Hybrid RRF search (v4.5.0) — fuses BM25 + vector when available.
    // Falls back to BM25-only when embeddings are not compiled or not initialized.
    // Clean query: strip EN/IT stop words so BM25 focuses on technical terms.
    let search_query = clean_query_for_search(query);

    // Extract original terms (pre-synonym) for D3 synonym noise filter (v5.0.1).
    // These are lowercased non-stop-word terms from the user query.
    let original_terms: Vec<String> = query
        .split(|c: char| c.is_whitespace() || c == '?' || c == '!' || c == ',')
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric() && c != '_')
                .to_ascii_lowercase()
        })
        .filter(|w| w.len() >= 2 && !STOP_WORDS.contains(&w.as_str()))
        .collect();

    let min_confidence = ctx.dna().context.min_confidence;
    if !search_query.is_empty() {
        if let Some(index) = ctx.get_extension::<SemanticIndex>() {
            // D40: Upcast to trait object for backend-agnostic method calls.
            let backend: &dyn SearchBackend = index.as_ref();

            // PageRank Module Authority (v4.8.0): inject scores into search index
            // so widely-imported modules get a mild ranking boost.
            // Computed lazily on first query, cached in SemanticIndex for subsequent calls.
            if !backend.has_pagerank_scores() {
                if let Some(graph) = ctx.get_extension::<CodeGraph>() {
                    let dep_graph = synapseed_architect::DependencyGraph::build(&graph);
                    let scores = dep_graph.pagerank_by_file();
                    let f32_scores: std::collections::HashMap<String, f32> =
                        scores.into_iter().map(|(k, v)| (k, v as f32)).collect();
                    debug!(
                        modules = f32_scores.len(),
                        "Whisper: PageRank scores injected"
                    );
                    backend.set_pagerank_scores(f32_scores);
                }
            }

            debug!(raw = query, cleaned = %search_query, "Whisper: cleaned query for search");

            let results = {
                #[cfg(feature = "embeddings")]
                {
                    use synapseed_search::embeddings::EmbeddingEngine;
                    use synapseed_search::vector_index::VectorIndex;

                    let vi = ctx.get_extension::<VectorIndex>();
                    let ee = ctx.get_extension::<EmbeddingEngine>();
                    if let (Some(ref vi), Some(ref ee)) = (vi, ee) {
                        debug!("Whisper: using Hybrid RRF (BM25 + vector)");
                        synapseed_search::hybrid::hybrid_search(backend, vi, ee, &search_query, 5)
                    } else {
                        debug!("Whisper: vector index not available, using BM25 only");
                        backend.search(&search_query, 5)
                    }
                }
                #[cfg(not(feature = "embeddings"))]
                {
                    backend.search(&search_query, 5)
                }
            };

            // Cognitive Ledger: recency boost for working-set files (v4.19.0)
            // Compendium mode: boost ≤ 1.2x, never penalize, never discard
            let mut results = results;
            if let Some(rec) =
                ctx.get_extension::<parking_lot::Mutex<synapseed_core::recorder::FlightRecorder>>()
            {
                let ws_subjects = rec.lock().working_set_subjects();
                if !ws_subjects.is_empty() {
                    for r in &mut results {
                        let boost = synapseed_core::ledger::MomentClassifier::recency_boost(
                            &r.symbol,
                            &r.file,
                            &ws_subjects,
                        );
                        r.score *= boost;
                    }
                    results.sort_by(|a, b| {
                        b.score
                            .partial_cmp(&a.score)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                }
            }

            for r in results {
                if r.score < min_confidence {
                    debug!(
                        symbol = %r.symbol, score = r.score, threshold = min_confidence,
                        "Whisper: dropping low-confidence search result"
                    );
                    continue;
                }

                // Synonym noise filter (v5.0.1 — D3 fix): verify the result
                // contains at least one original query term in its symbol name,
                // file path, or signature. Results matched ONLY through
                // synonym expansion are likely noise.
                let sym_lower = r.symbol.to_ascii_lowercase();
                let file_lower = r.file.to_ascii_lowercase();
                let sig_lower = r.signature.to_ascii_lowercase();
                let has_original = original_terms.iter().any(|t| {
                    sym_lower.contains(t.as_str())
                        || file_lower.contains(t.as_str())
                        || sig_lower.contains(t.as_str())
                });
                if !has_original && !original_terms.is_empty() {
                    debug!(
                        symbol = %r.symbol, score = r.score,
                        "Whisper: dropping synonym-only result (no original term match)"
                    );
                    continue;
                }

                targets.push(Target {
                    kind: TargetKind::Symbol,
                    name: r.symbol.clone(),
                    file_path: Some(r.file.clone()),
                    line_start: Some(r.line_start as usize),
                    score: Some(r.score),
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
                        score: None,
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
        // Collect indices of test targets to avoid cloning (v5.0.0 speed opt).
        let test_indices: Vec<usize> = targets
            .iter()
            .enumerate()
            .filter(|(_, t)| t.file_path.as_deref().is_some_and(is_test_path))
            .map(|(i, _)| i)
            .collect();

        for &idx in &test_indices {
            let fp = match &targets[idx].file_path {
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
                                score: None,
                            });
                        }
                        break; // Found the twin, stop searching candidates
                    }
                }
            }
        }

        // Pass 5: Call Graph Lite — extract identifiers from test bodies
        if let Some(ref g) = graph {
            for &idx in &test_indices {
                let fp = match &targets[idx].file_path {
                    Some(p) => p,
                    None => continue,
                };
                let target_name = targets[idx].name.clone();
                let abs_path = ctx.project_root().join(fp);
                if let Ok(content) = std::fs::read_to_string(&abs_path) {
                    let identifiers = extract_call_identifiers(&content, &target_name);
                    debug!(
                        test_fn = %target_name, ids = ?identifiers,
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
                                score: None,
                            });
                        }
                    }
                }
            }
        }
    }

    // ── Pass 5.5: Module Focus Pre-filter (D20 fix) ──────────────────
    // Before the Config Resolver reads Python files, trim obvious outlier
    // targets so Pass 6 doesn't waste I/O on files that the downstream
    // Coherence Gate (Stage 5 of the pipeline) would discard anyway.
    //
    // This is intentionally lightweight: count module prefixes, keep
    // targets in the top-2 modules + any without a file path.  Only
    // activates when we have enough targets (>6) to justify filtering.
    if targets.len() > 6 {
        let mut module_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for t in &targets {
            let prefix = t.file_path.as_deref().map_or_else(String::new, |fp| {
                let parts: Vec<&str> = fp.split('/').collect();
                if parts.len() >= 3 {
                    format!("{}/{}", parts[0], parts[1])
                } else if parts.len() == 2 {
                    parts[0].to_string()
                } else {
                    fp.to_string()
                }
            });
            *module_counts.entry(prefix).or_default() += 1;
        }

        // Keep the top-2 modules by target count (matching Coherence Gate logic)
        let mut ranked: Vec<_> = module_counts.into_iter().collect();
        ranked.sort_by_key(|r| std::cmp::Reverse(r.1));
        let keep_prefixes: HashSet<String> = ranked.into_iter().take(2).map(|(p, _)| p).collect();

        let before = targets.len();
        targets.retain(|t| {
            let prefix = t.file_path.as_deref().map_or_else(String::new, |fp| {
                let parts: Vec<&str> = fp.split('/').collect();
                if parts.len() >= 3 {
                    format!("{}/{}", parts[0], parts[1])
                } else if parts.len() == 2 {
                    parts[0].to_string()
                } else {
                    fp.to_string()
                }
            });
            keep_prefixes.contains(&prefix)
        });
        if targets.len() < before {
            debug!(
                before,
                after = targets.len(),
                modules = ?keep_prefixes,
                "Whisper: Module Focus Pre-filter trimmed outlier targets"
            );
        }
    }

    // ── Pass 6: Python Config String Resolver (v4.2.0) ──────────────────
    // Resolves dotted string references in Python configuration files
    // (e.g., Django's MIDDLEWARE = ["django.middleware.security.SecurityMiddleware"]).
    // Extracts the last dotted component (class name) and looks it up in the graph.
    {
        let has_python_targets = targets
            .iter()
            .any(|t| t.file_path.as_deref().is_some_and(|fp| fp.ends_with(".py")));

        if has_python_targets {
            if let Some(graph) = ctx.get_extension::<CodeGraph>() {
                let python_files: Vec<String> = targets
                    .iter()
                    .filter_map(|t| t.file_path.as_ref())
                    .filter(|fp| fp.ends_with(".py"))
                    .cloned()
                    .collect();

                for file_path in &python_files {
                    let abs_path = ctx.project_root().join(file_path);
                    if let Ok(content) = std::fs::read_to_string(&abs_path) {
                        for class_name in extract_dotted_references(&content) {
                            for sym in graph.lookup(&class_name).into_iter().take(2) {
                                if matches!(sym.kind, SymbolKind::Import | SymbolKind::Variable) {
                                    continue;
                                }
                                if targets.iter().any(|t| t.name == sym.name) {
                                    continue; // already present
                                }
                                debug!(
                                    config_ref = %class_name, symbol = %sym.name,
                                    "Whisper: Python Config Resolver found symbol"
                                );
                                targets.push(Target {
                                    kind: TargetKind::Symbol,
                                    name: sym.name.clone(),
                                    file_path: Some(sym.file_path.clone()),
                                    line_start: Some(sym.line_start),
                                    score: None,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Pass 7: Reverse Test Lookup (v4.6.0) — "Il Ponte della Verità" ──
    // For each source symbol found by earlier passes, search for test functions
    // that exercise it. Tests are working usage examples — for an LLM, seeing
    // `assert_eq!(handler(req), expected)` is more valuable than 100 lines of
    // abstract implementation. This is the mirror of Passes 4-5 (test→source):
    // Pass 7 goes source→test.
    {
        let source_symbols: Vec<String> = targets
            .iter()
            .filter(|t| matches!(t.kind, TargetKind::Symbol))
            .filter(|t| !t.file_path.as_deref().is_some_and(is_test_path))
            .filter(|t| t.name.len() >= 4) // skip short names (noise)
            .map(|t| t.name.clone())
            .collect();

        if !source_symbols.is_empty() {
            if let Some(index) = ctx.get_extension::<SemanticIndex>() {
                let mut test_injections = 0usize;
                const MAX_TEST_INJECTIONS: usize = 3;

                for sym_name in &source_symbols {
                    if test_injections >= MAX_TEST_INJECTIONS {
                        break;
                    }
                    // D40: Use trait object for backend-agnostic method calls.
                    let backend: &dyn SearchBackend = index.as_ref();
                    // Over-fetch from BM25: source files rank higher, so we need
                    // extra results to find test files that reference this symbol.
                    let results = backend.search(sym_name, 10);
                    for r in results {
                        if test_injections >= MAX_TEST_INJECTIONS {
                            break;
                        }
                        if !is_test_path(&r.file) {
                            continue;
                        }
                        // Dedup: skip if this exact (name, file) is already a target
                        if targets
                            .iter()
                            .any(|t| t.name == r.symbol && t.file_path.as_deref() == Some(&r.file))
                        {
                            continue;
                        }
                        debug!(
                            source_sym = %sym_name,
                            test_fn = %r.symbol,
                            test_file = %r.file,
                            score = r.score,
                            "Whisper: Reverse Test Lookup found exercising test"
                        );
                        targets.push(Target {
                            kind: TargetKind::Symbol,
                            name: r.symbol.clone(),
                            file_path: Some(r.file.clone()),
                            line_start: Some(r.line_start as usize),
                            score: Some(r.score),
                        });
                        test_injections += 1;
                    }
                }

                if test_injections > 0 {
                    debug!(
                        injected = test_injections,
                        "Whisper: Reverse Test Lookup complete"
                    );
                }
            }
        }
    }

    // ── Sort First, Cut Later (v4.12.0) ────────────────────────────────
    // 1. Sort by score DESC so highest-quality targets come first.
    //    Targets without scores (non-search passes) are ordered by source-first
    //    heuristic within their group.
    // 2. HashSet dedup: keeps the FIRST occurrence (= highest score).
    //    This fixes the old dedup_by which only caught consecutive duplicates.
    targets.sort_by(|a, b| {
        // Primary: score descending (None after Some)
        let score_cmp = b.score.unwrap_or(-1.0).total_cmp(&a.score.unwrap_or(-1.0));
        if score_cmp != std::cmp::Ordering::Equal {
            return score_cmp;
        }
        // Secondary: source > test > vendor
        let a_order = source_order(a.file_path.as_deref().unwrap_or(""));
        let b_order = source_order(b.file_path.as_deref().unwrap_or(""));
        a_order.cmp(&b_order)
    });

    // HashSet dedup by (name, file_path) — keeps highest-scored version
    {
        let mut seen = HashSet::new();
        targets.retain(|t| {
            let key = (t.name.clone(), t.file_path.clone().unwrap_or_default());
            seen.insert(key)
        });
    }

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

/// Source-first ordering: source=0, test=1, vendor=2.
fn source_order(path: &str) -> u8 {
    if is_vendor_path(path) {
        2
    } else if is_test_path(path) {
        1
    } else {
        0
    }
}

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
fn derive_source_paths(test_path: &str) -> Vec<String> {
    let path = std::path::Path::new(test_path);
    let file_name = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    // Strip test_ prefix and _test / .test suffix
    let base = file_name.strip_prefix("test_").unwrap_or(file_name);
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

/// Extract class names from dotted string references in Python config files.
///
/// Looks for quoted dotted paths like `"django.middleware.security.SecurityMiddleware"`
/// and extracts the last component (`SecurityMiddleware`), which is typically a class name.
///
/// Only returns names that look like class names (start with uppercase, ≥3 chars).
fn extract_dotted_references(source: &str) -> Vec<String> {
    static DOTTED_RE: LazyLock<Regex> = LazyLock::new(|| {
        // Match quoted dotted paths: "a.b.c.ClassName" or 'a.b.c.ClassName'
        Regex::new(r#"["']([a-zA-Z_]\w*(?:\.[a-zA-Z_]\w*){2,})["']"#).expect("valid regex")
    });

    let mut names = Vec::new();
    for cap in DOTTED_RE.captures_iter(source) {
        let dotted = &cap[1];
        if let Some(last) = dotted.rsplit('.').next() {
            // Only include if it looks like a class name (PascalCase, ≥3 chars)
            if last.len() >= 3
                && last.starts_with(|c: char| c.is_uppercase())
                && !names.contains(&last.to_string())
            {
                names.push(last.to_string());
            }
        }
    }
    names
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

    // ── Python Config String Resolver tests (v4.2.0) ────────────────

    #[test]
    fn test_extract_dotted_references_django_middleware() {
        let source = r#"
MIDDLEWARE = [
    "django.middleware.security.SecurityMiddleware",
    "django.middleware.common.CommonMiddleware",
    "django.middleware.csrf.CsrfViewMiddleware",
]
"#;
        let refs = extract_dotted_references(source);
        assert_eq!(
            refs,
            vec![
                "SecurityMiddleware",
                "CommonMiddleware",
                "CsrfViewMiddleware",
            ]
        );
    }

    #[test]
    fn test_extract_dotted_references_single_quotes() {
        let source = "INSTALLED_APPS = ['django.contrib.auth.AuthConfig']";
        let refs = extract_dotted_references(source);
        assert_eq!(refs, vec!["AuthConfig"]);
    }

    #[test]
    fn test_extract_dotted_references_ignores_non_class() {
        // lowercase last component → not a class name → excluded
        let source = r#""os.path.join""#;
        let refs = extract_dotted_references(source);
        assert!(refs.is_empty());
    }

    #[test]
    fn test_extract_dotted_references_dedup() {
        let source = r#"
a = "pkg.module.Handler"
b = "pkg.module.Handler"
"#;
        let refs = extract_dotted_references(source);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], "Handler");
    }

    #[test]
    fn test_extract_dotted_references_too_few_dots() {
        // Need at least 3 components (2 dots) to be a dotted path
        let source = r#""os.path""#;
        let refs = extract_dotted_references(source);
        assert!(refs.is_empty());
    }
}
