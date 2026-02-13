//! Semantic search indexer — builds and queries the Tantivy index.
//!
//! Extracts doc comments and body snippets from source files,
//! then indexes them alongside AST symbol metadata.

use std::path::Path;
use std::sync::Arc;

use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::Value;
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};
use tracing::{debug, info, warn};

use synapseed_core::error::safe_resolve_path;
use synapseed_core::symbol::{FileStructure, Visibility};

use crate::schema::{build_schema, fields_from_schema, SearchFields};

/// Result of a semantic search query.
#[derive(Debug, serde::Serialize)]
pub struct SearchResult {
    pub score: f32,
    pub file: String,
    pub symbol: String,
    pub kind: String,
    pub line_start: u64,
    pub line_end: u64,
    pub signature: String,
    pub snippet: String,
    pub last_modified_epoch: u64,
}

/// The semantic search index — wraps Tantivy in a thread-safe handle.
pub struct SemanticIndex {
    index: Index,
    fields: SearchFields,
    reader: IndexReader,
    writer: Arc<std::sync::Mutex<IndexWriter>>,
    temporal_decay_lambda: f64,
    /// Module authority scores from PageRank (v4.8.0).
    /// Key = file_path (absolute), value = normalized [0, 1] score.
    pagerank_scores: parking_lot::RwLock<std::collections::HashMap<String, f32>>,
}

impl SemanticIndex {
    /// Create a new in-memory index.
    pub fn new() -> Result<Self, tantivy::TantivyError> {
        let (schema, fields) = build_schema();
        let index = Index::create_in_ram(schema);

        let writer = index.writer(15_000_000)?; // 15 MB heap
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        Ok(Self {
            index,
            fields,
            reader,
            writer: Arc::new(std::sync::Mutex::new(writer)),
            temporal_decay_lambda: 0.05,
            pagerank_scores: parking_lot::RwLock::new(std::collections::HashMap::new()),
        })
    }

    /// Open an existing disk-based index, or create a new one.
    /// Falls back gracefully on schema mismatch by recreating the index.
    pub fn open_or_create(index_dir: &Path) -> Result<Self, tantivy::TantivyError> {
        let (schema, fields) = build_schema();

        if !index_dir.exists() {
            std::fs::create_dir_all(index_dir).map_err(|e| {
                tantivy::TantivyError::SystemError(format!("Failed to create index directory: {e}"))
            })?;
        }

        let (index, fields) = match Index::open_in_dir(index_dir) {
            Ok(idx) => {
                // Verify schema compatibility
                match fields_from_schema(&idx.schema()) {
                    Some(recovered) => {
                        info!("Search: Opened existing disk index");
                        (idx, recovered)
                    }
                    None => {
                        warn!("Search: Schema mismatch, recreating disk index");
                        drop(idx);
                        let _ = std::fs::remove_dir_all(index_dir);
                        std::fs::create_dir_all(index_dir).map_err(|e| {
                            tantivy::TantivyError::SystemError(format!(
                                "Failed to recreate index directory: {e}"
                            ))
                        })?;
                        let idx = Index::create_in_dir(index_dir, schema)?;
                        (idx, fields)
                    }
                }
            }
            Err(_) => {
                info!("Search: Creating new disk index");
                let idx = Index::create_in_dir(index_dir, schema)?;
                (idx, fields)
            }
        };

        let writer = index.writer(15_000_000)?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        Ok(Self {
            index,
            fields,
            reader,
            writer: Arc::new(std::sync::Mutex::new(writer)),
            temporal_decay_lambda: 0.05,
            pagerank_scores: parking_lot::RwLock::new(std::collections::HashMap::new()),
        })
    }

    /// Set the temporal decay parameter λ for search ranking.
    ///
    /// Higher values penalize older results more aggressively.
    /// Default: 0.05 (half-life ≈ 14 days).
    pub fn set_temporal_decay(&mut self, lambda: f64) {
        self.temporal_decay_lambda = lambda;
    }

    /// Inject module-level PageRank scores for search ranking (v4.8.0).
    ///
    /// Scores are keyed by file_path and valued in [0.0, 1.0].
    /// Applied as a multiplicative boost in `search()`: `1.0 + score × 0.5`.
    pub fn set_pagerank_scores(&self, scores: std::collections::HashMap<String, f32>) {
        *self.pagerank_scores.write() = scores;
    }

    /// Whether PageRank scores have been injected.
    pub fn has_pagerank_scores(&self) -> bool {
        !self.pagerank_scores.read().is_empty()
    }

    /// Index all symbols from a CodeGraph snapshot.
    /// Reads source files to extract doc comments and body snippets.
    pub fn index_all(&self, files: &[FileStructure], project_root: &Path) -> usize {
        let mut count = 0;

        let mut writer = match self.writer.lock() {
            Ok(w) => w,
            Err(e) => {
                warn!(error = %e, "Search: Failed to acquire writer lock");
                return 0;
            }
        };

        for file in files {
            // Read source file for doc comment + body extraction
            let source = self.read_source(file, project_root);
            let mtime = self.file_mtime(file, project_root);

            for sym in &file.symbols {
                let mut doc_comment = source
                    .as_ref()
                    .map(|s| extract_doc_comment(s, sym.line_start))
                    .unwrap_or_default();

                // CamelCase Index Expansion (v4.1.0): append split components
                // to doc_comment so BM25 can match partial name queries.
                // "MomentumEngine" → " Momentum Engine" appended to doc_comment.
                let camel_expansion = split_camel_case_for_index(&sym.name);
                if !camel_expansion.is_empty() {
                    if !doc_comment.is_empty() {
                        doc_comment.push(' ');
                    }
                    doc_comment.push_str(&camel_expansion);
                }

                let body_snippet = source
                    .as_ref()
                    .map(|s| extract_body_snippet(s, sym.line_start, sym.line_end))
                    .unwrap_or_default();

                let kind_str = format!("{:?}", sym.kind);
                let sig = sym.signature.clone().unwrap_or_default();
                let vis_str = visibility_to_str(sym.visibility);

                match writer.add_document(doc!(
                    self.fields.file_path => file.path.clone(),
                    self.fields.symbol_name => sym.name.clone(),
                    self.fields.kind => kind_str,
                    self.fields.signature => sig,
                    self.fields.doc_comment => doc_comment,
                    self.fields.body_snippet => body_snippet,
                    self.fields.line_start => sym.line_start as u64,
                    self.fields.line_end => sym.line_end as u64,
                    self.fields.last_modified_epoch => mtime,
                    self.fields.visibility => vis_str,
                )) {
                    Ok(_) => count += 1,
                    Err(e) => warn!(error = %e, symbol = %sym.name, "Search: Failed to add document"),
                }
            }
        }

        if let Err(e) = writer.commit() {
            warn!(error = %e, "Search: Failed to commit index");
        }

        // Force reader reload so searches see the new segments immediately.
        // Without this, OnCommitWithDelay introduces ~500ms lag and callers
        // racing right after SearchReady would get stale (empty) results.
        if let Err(e) = self.reader.reload() {
            warn!(error = %e, "Search: Failed to reload reader after commit");
        }

        info!(symbols = count, "Search: Indexed symbols");
        count
    }

    /// Incremental update: remove all docs for a file path, then re-index.
    pub fn reindex_file(&self, file: &FileStructure, project_root: &Path) -> usize {
        let mut writer = match self.writer.lock() {
            Ok(w) => w,
            Err(_) => return 0,
        };

        // Delete all existing docs for this file path
        let term = Term::from_field_text(self.fields.file_path, &file.path);
        writer.delete_term(term);

        let source = self.read_source(file, project_root);
        let mtime = self.file_mtime(file, project_root);
        let mut count = 0;

        for sym in &file.symbols {
            let mut doc_comment = source
                .as_ref()
                .map(|s| extract_doc_comment(s, sym.line_start))
                .unwrap_or_default();

            // CamelCase Index Expansion (v4.1.0)
            let camel_expansion = split_camel_case_for_index(&sym.name);
            if !camel_expansion.is_empty() {
                if !doc_comment.is_empty() {
                    doc_comment.push(' ');
                }
                doc_comment.push_str(&camel_expansion);
            }

            let body_snippet = source
                .as_ref()
                .map(|s| extract_body_snippet(s, sym.line_start, sym.line_end))
                .unwrap_or_default();

            let kind_str = format!("{:?}", sym.kind);
            let sig = sym.signature.clone().unwrap_or_default();
            let vis_str = visibility_to_str(sym.visibility);

            match writer.add_document(doc!(
                self.fields.file_path => file.path.clone(),
                self.fields.symbol_name => sym.name.clone(),
                self.fields.kind => kind_str,
                self.fields.signature => sig,
                self.fields.doc_comment => doc_comment,
                self.fields.body_snippet => body_snippet,
                self.fields.line_start => sym.line_start as u64,
                self.fields.line_end => sym.line_end as u64,
                self.fields.last_modified_epoch => mtime,
                self.fields.visibility => vis_str,
            )) {
                Ok(_) => count += 1,
                Err(e) => warn!(error = %e, symbol = %sym.name, "Search: Failed to add document"),
            }
        }

        if let Err(e) = writer.commit() {
            warn!(error = %e, "Search: Failed to commit incremental update");
        }
        if let Err(e) = self.reader.reload() {
            warn!(error = %e, "Search: Failed to reload reader after reindex");
        }

        debug!(file = %file.path, symbols = count, "Search: Reindexed file");
        count
    }

    /// Remove all docs for a deleted file.
    pub fn remove_file(&self, path: &str) {
        if let Ok(mut writer) = self.writer.lock() {
            let term = Term::from_field_text(self.fields.file_path, path);
            writer.delete_term(term);
            if let Err(e) = writer.commit() {
                warn!(error = %e, file = path, "Search: Failed to commit file removal");
            }
            if let Err(e) = self.reader.reload() {
                warn!(error = %e, file = path, "Search: Failed to reload reader after removal");
            }
            debug!(file = path, "Search: Removed file from index");
        }
    }

    /// Semantic search with per-field boosts: symbol_name (3x) > doc_comment (2x) > body (1.5x) > signature (1x).
    pub fn search(&self, query_str: &str, limit: usize) -> Vec<SearchResult> {
        let searcher = self.reader.searcher();

        // Over-retrieve 4× then re-rank with path/source/temporal boosts.
        // This lets path-relevant results (e.g., scheduler/multi_thread/worker.rs)
        // surface even if their BM25 name-score is lower than generic matches.
        let retrieval_limit = limit * 4;

        let mut query_parser = QueryParser::for_index(
            &self.index,
            vec![
                self.fields.symbol_name,
                self.fields.doc_comment,
                self.fields.signature,
                self.fields.body_snippet,
            ],
        );

        // Per-field boosts: name > doc_comment > body_snippet > signature.
        // body_snippet raised to 1.5 (v3.9.4): domain keywords often appear in docstrings
        // and early body lines but NOT in the symbol name (e.g. "chunk" in iter_content).
        query_parser.set_field_boost(self.fields.symbol_name, 3.0);
        query_parser.set_field_boost(self.fields.doc_comment, 2.0);
        query_parser.set_field_boost(self.fields.signature, 1.0);
        query_parser.set_field_boost(self.fields.body_snippet, 1.5);

        // Try to parse the user query. If it fails (bad syntax), fall back to a simpler approach.
        let query = match query_parser.parse_query(query_str) {
            Ok(q) => q,
            Err(_) => {
                // Escape the query and try again
                let escaped = query_str.replace([':', '(', ')'], " ");
                match query_parser.parse_query(&escaped) {
                    Ok(q) => q,
                    Err(e) => {
                        warn!(error = %e, query = query_str, "Search: Failed to parse query");
                        return Vec::new();
                    }
                }
            }
        };

        let top_docs = match searcher.search(&query, &TopDocs::with_limit(retrieval_limit)) {
            Ok(results) => results,
            Err(e) => {
                warn!(error = %e, "Search: Query execution failed");
                return Vec::new();
            }
        };

        let mut results = Vec::new();
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = match searcher.doc(doc_address) {
                Ok(d) => d,
                Err(_) => continue,
            };

            let get_text = |field: tantivy::schema::Field| -> String {
                doc.get_first(field)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            };

            let get_u64 = |field: tantivy::schema::Field| -> u64 {
                doc.get_first(field).and_then(|v| v.as_u64()).unwrap_or(0)
            };

            let last_modified = get_u64(self.fields.last_modified_epoch);
            let age_days = if now_secs > last_modified {
                (now_secs - last_modified) as f64 / 86400.0
            } else {
                0.0
            };
            let temporal_boost = 0.7 + 0.3 * (-self.temporal_decay_lambda * age_days).exp();

            // Source-First (v3.9.3, tuned v4.1.0): boost source, penalize test/vendor.
            // Test penalty tightened from 0.5 to 0.3 — test function names are
            // inherently keyword-dense and unfairly dominate BM25 rankings.
            let file_path = get_text(self.fields.file_path);
            let source_boost: f32 = if is_vendor_path(&file_path) {
                0.1
            } else if is_test_path(&file_path) {
                0.3
            } else {
                1.5
            };

            // Symbol Specificity Boost (v4.1.0): longer/unique names are more
            // likely to be what the user wants. "MomentumEngine" (15 chars) is
            // more specific than "build" (5 chars).
            let symbol_name = get_text(self.fields.symbol_name);
            let specificity_boost: f32 = if symbol_name.len() >= 12 {
                1.3
            } else if symbol_name.len() >= 8 {
                1.1
            } else {
                1.0
            };

            // Interface Boost (v4.4.0): trait/interface definitions are high-value
            // architectural symbols that define contracts. Boosting them ensures
            // FromRequest, MiddlewareMixin, ServiceFactory etc. surface over
            // concrete implementations in BM25 rankings.
            let kind_value = get_text(self.fields.kind);
            let interface_boost: f32 = if kind_value == "Interface" {
                1.4
            } else {
                1.0
            };

            // Path-relevance boost (v3.10.2): if query terms appear in the
            // file path, the result is likely more relevant.
            let path_lower = file_path.to_ascii_lowercase();
            let path_matches = query_str
                .split_whitespace()
                .filter(|t| t.len() >= 3)
                .filter(|t| path_lower.contains(&t.to_ascii_lowercase()))
                .count();
            let path_boost: f32 = match path_matches {
                0 => 1.0,
                1 => 1.5,
                2 => 2.5,
                _ => 3.0,
            };

            // Module Authority Boost (v4.8.0 — PageRank): symbols from widely-imported
            // modules are foundational and should rank higher. Score range: 1.0–1.5.
            let pagerank_boost: f32 = {
                let pr = self.pagerank_scores.read();
                pr.get(&file_path).map(|s| 1.0 + s * 0.5).unwrap_or(1.0)
            };

            // Visibility Boost (v4.9.0 — Public API Prioritization): public API
            // symbols rank higher than internal implementation details.
            // Fixes the fidelity defect where `Server` (internal) outranks
            // `HttpServer` (public API), causing LLM hallucination.
            let visibility_str = get_text(self.fields.visibility);
            let visibility_boost: f32 = match visibility_str.as_str() {
                "public" => 1.5,
                "crate" => 1.0,
                "super" => 0.8,
                "private" => 0.6,
                _ => 1.0, // "unknown" or legacy docs without visibility
            };

            results.push(SearchResult {
                score: score * temporal_boost as f32 * source_boost * path_boost * specificity_boost * interface_boost * pagerank_boost * visibility_boost,
                file: file_path,
                symbol: get_text(self.fields.symbol_name),
                kind: get_text(self.fields.kind),
                line_start: get_u64(self.fields.line_start),
                line_end: get_u64(self.fields.line_end),
                signature: get_text(self.fields.signature),
                snippet: get_text(self.fields.body_snippet),
                last_modified_epoch: last_modified,
            });
        }

        // Re-sort by adjusted score, then truncate to requested limit.
        results.sort_by(|a, b| b.score.total_cmp(&a.score));
        results.truncate(limit);
        results
    }

    /// Try to read the source file, resolving relative paths against project root.
    /// Returns `None` if the path escapes the project root (path-traversal guard).
    fn read_source(&self, file: &FileStructure, project_root: &Path) -> Option<String> {
        let safe_path = safe_resolve_path(project_root, &file.path).ok()?;
        std::fs::read_to_string(&safe_path).ok()
    }

    /// Get file modification time as Unix epoch seconds.
    /// Returns 0 if the path escapes the project root (path-traversal guard).
    fn file_mtime(&self, file: &FileStructure, project_root: &Path) -> u64 {
        let path = match safe_resolve_path(project_root, &file.path) {
            Ok(p) => p,
            Err(_) => return 0,
        };

        std::fs::metadata(&path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

// ── Visibility Helpers ──────────────────────────────────────────────

/// Convert a Visibility option to its string representation for Tantivy storage.
fn visibility_to_str(vis: Option<Visibility>) -> &'static str {
    match vis {
        Some(Visibility::Public) => "public",
        Some(Visibility::Crate) => "crate",
        Some(Visibility::Super) => "super",
        Some(Visibility::Private) => "private",
        None => "unknown",
    }
}

// ── Doc Comment Extraction ──────────────────────────────────────────

/// Extract doc comments above a symbol's start line.
/// Handles `///`, `//!`, and `/** ... */` patterns.
fn extract_doc_comment(source: &str, line_start: usize) -> String {
    let lines: Vec<&str> = source.lines().collect();
    if line_start == 0 || line_start > lines.len() {
        return String::new();
    }

    let mut doc_lines = Vec::new();
    let mut i = line_start.saturating_sub(2); // 0-indexed, line before symbol

    // Walk backwards collecting doc comment lines
    loop {
        let trimmed = lines.get(i).map(|l| l.trim()).unwrap_or("");

        if trimmed.starts_with("///") {
            doc_lines.push(trimmed.trim_start_matches("///").trim());
        } else if trimmed.starts_with("//!") {
            doc_lines.push(trimmed.trim_start_matches("//!").trim());
        } else if trimmed.starts_with("*")
            || trimmed.starts_with("/**")
            || trimmed.starts_with("*/")
        {
            // Multi-line doc comment
            let cleaned = trimmed
                .trim_start_matches("/**")
                .trim_start_matches("*/")
                .trim_start_matches('*')
                .trim();
            if !cleaned.is_empty() {
                doc_lines.push(cleaned);
            }
        } else if trimmed.starts_with('#') {
            // Python docstring-like comment or decorator — stop
            break;
        } else if trimmed.is_empty() && !doc_lines.is_empty() {
            // Blank line after collecting some comments — we're past the doc block
            break;
        } else if !trimmed.is_empty() {
            break;
        }

        if i == 0 {
            break;
        }
        i -= 1;
    }

    doc_lines.reverse();
    doc_lines.join(" ")
}

/// Extract the first lines of a symbol's body as a snippet for BM25 indexing.
/// Captures up to 30 lines to include docstrings and early body content,
/// which often contain domain-specific keywords not present in the symbol name.
/// Expanded from 15 to 30 in v4.0.0 (The Deep Index) to capture constants and
/// logic deeper in function bodies — addresses grounding experiment Q4/Q10 misses.
fn extract_body_snippet(source: &str, line_start: usize, line_end: usize) -> String {
    let lines: Vec<&str> = source.lines().collect();
    if line_start == 0 || line_start > lines.len() {
        return String::new();
    }

    let start = line_start.saturating_sub(1); // 0-indexed
    let end = (start + 30)
        .min(line_end.saturating_sub(1) + 1)
        .min(lines.len());

    lines[start..end].join("\n")
}

/// Returns true if the path looks like a test file.
/// Used by Source-First scoring to penalize test results (v3.9.3).
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

/// Split a CamelCase symbol name into components for index expansion.
///
/// Returns a space-separated string of both original-case and lowercase
/// components to maximize BM25 coverage.
///
/// "MomentumEngine" → "Momentum Engine momentum engine"
/// "CodeGraph" → "Code Graph code graph"
/// "build_context" → "" (snake_case → no split)
fn split_camel_case_for_index(name: &str) -> String {
    // Only split if the string contains mixed case
    if name.contains('_')
        || name.chars().all(|c| c.is_uppercase() || !c.is_alphabetic())
        || name.chars().all(|c| c.is_lowercase() || !c.is_alphabetic())
    {
        return String::new();
    }

    let chars: Vec<char> = name.chars().collect();
    let mut parts = Vec::new();
    let mut start = 0;

    for i in 1..chars.len() {
        let split_here = (chars[i].is_uppercase() && chars[i - 1].is_lowercase())
            || (i + 1 < chars.len()
                && chars[i].is_uppercase()
                && chars[i + 1].is_lowercase()
                && chars[i - 1].is_uppercase());

        if split_here {
            let part: String = chars[start..i].iter().collect();
            if part.len() >= 2 {
                parts.push(part);
            }
            start = i;
        }
    }

    let part: String = chars[start..].iter().collect();
    if part.len() >= 2 {
        parts.push(part);
    }

    if parts.len() <= 1 {
        return String::new();
    }

    // Emit both original-case and lowercase for unambiguous BM25 matching
    let mut tokens = Vec::new();
    for p in &parts {
        tokens.push(p.clone());
        let lower = p.to_lowercase();
        if lower != *p {
            tokens.push(lower);
        }
    }
    tokens.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_doc_comment_rust() {
        let source = r#"
/// Authenticates a user against the database.
/// Returns a JWT token on success.
pub fn authenticate_user(credentials: &Credentials) -> Result<Token> {
    // ...
}
"#;
        let comment = extract_doc_comment(source, 4);
        assert!(comment.contains("Authenticates a user"));
        assert!(comment.contains("JWT token"));
    }

    #[test]
    fn test_extract_body_snippet() {
        // With 30-line capture, lines 2-7 (6 lines) should all be included
        let source = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8";
        let snippet = extract_body_snippet(source, 2, 7);
        assert!(snippet.starts_with("line2"));
        assert!(snippet.contains("line6"));
        assert!(snippet.contains("line7")); // included within 30-line window
        assert!(!snippet.contains("line8")); // beyond line_end=7

        // Test truncation at 30 lines
        let long_source = (1..=40).map(|i| format!("line{i}")).collect::<Vec<_>>().join("\n");
        let snippet = extract_body_snippet(&long_source, 1, 40);
        assert!(snippet.contains("line30")); // 30th line included
        assert!(!snippet.contains("line31")); // beyond 30-line window
    }

    #[test]
    fn test_empty_doc_comment() {
        let source = "fn foo() {}\n";
        let comment = extract_doc_comment(source, 1);
        assert!(comment.is_empty());
    }

    // ── Source-First Scoring tests (v3.9.3) ──────────────────────────

    #[test]
    fn test_is_test_path_positive() {
        assert!(is_test_path("tests/test_lowlevel.py"));
        assert!(is_test_path("test_requests.py"));
        assert!(is_test_path("src/models_test.go"));
        assert!(is_test_path("auth.test.ts"));
        assert!(is_test_path("spec/handler_spec.rb"));
        assert!(is_test_path("__tests__/utils.spec.js"));
    }

    #[test]
    fn test_is_test_path_negative() {
        assert!(!is_test_path("src/requests/models.py"));
        assert!(!is_test_path("src/main.rs"));
        assert!(!is_test_path("lib/auth.ts"));
        assert!(!is_test_path("src/attestation/verify.rs"));
    }

    // ── CamelCase Index Expansion tests (v4.1.0) ────────────────────

    #[test]
    fn test_split_camel_case_for_index() {
        let result = split_camel_case_for_index("MomentumEngine");
        assert!(result.contains("Momentum"));
        assert!(result.contains("Engine"));
        assert!(result.contains("momentum"));
        assert!(result.contains("engine"));
    }

    #[test]
    fn test_split_camel_case_for_index_three_parts() {
        let result = split_camel_case_for_index("SmartContextInput");
        assert!(result.contains("Smart"));
        assert!(result.contains("Context"));
        assert!(result.contains("Input"));
        assert!(result.contains("smart"));
        assert!(result.contains("context"));
        assert!(result.contains("input"));
    }

    #[test]
    fn test_split_camel_case_for_index_snake_case_noop() {
        assert!(split_camel_case_for_index("build_context").is_empty());
        assert!(split_camel_case_for_index("extract_targets").is_empty());
    }

    #[test]
    fn test_split_camel_case_for_index_single_word_noop() {
        assert!(split_camel_case_for_index("tokio").is_empty());
        assert!(split_camel_case_for_index("HTML").is_empty());
    }
}
