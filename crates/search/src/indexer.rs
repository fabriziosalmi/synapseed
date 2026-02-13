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
use synapseed_core::symbol::FileStructure;

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
        })
    }

    /// Set the temporal decay parameter λ for search ranking.
    ///
    /// Higher values penalize older results more aggressively.
    /// Default: 0.05 (half-life ≈ 14 days).
    pub fn set_temporal_decay(&mut self, lambda: f64) {
        self.temporal_decay_lambda = lambda;
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
                let doc_comment = source
                    .as_ref()
                    .map(|s| extract_doc_comment(s, sym.line_start))
                    .unwrap_or_default();

                let body_snippet = source
                    .as_ref()
                    .map(|s| extract_body_snippet(s, sym.line_start, sym.line_end))
                    .unwrap_or_default();

                let kind_str = format!("{:?}", sym.kind);
                let sig = sym.signature.clone().unwrap_or_default();

                let _ = writer.add_document(doc!(
                    self.fields.file_path => file.path.clone(),
                    self.fields.symbol_name => sym.name.clone(),
                    self.fields.kind => kind_str,
                    self.fields.signature => sig,
                    self.fields.doc_comment => doc_comment,
                    self.fields.body_snippet => body_snippet,
                    self.fields.line_start => sym.line_start as u64,
                    self.fields.line_end => sym.line_end as u64,
                    self.fields.last_modified_epoch => mtime,
                ));

                count += 1;
            }
        }

        if let Err(e) = writer.commit() {
            warn!(error = %e, "Search: Failed to commit index");
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
            let doc_comment = source
                .as_ref()
                .map(|s| extract_doc_comment(s, sym.line_start))
                .unwrap_or_default();

            let body_snippet = source
                .as_ref()
                .map(|s| extract_body_snippet(s, sym.line_start, sym.line_end))
                .unwrap_or_default();

            let kind_str = format!("{:?}", sym.kind);
            let sig = sym.signature.clone().unwrap_or_default();

            let _ = writer.add_document(doc!(
                self.fields.file_path => file.path.clone(),
                self.fields.symbol_name => sym.name.clone(),
                self.fields.kind => kind_str,
                self.fields.signature => sig,
                self.fields.doc_comment => doc_comment,
                self.fields.body_snippet => body_snippet,
                self.fields.line_start => sym.line_start as u64,
                self.fields.line_end => sym.line_end as u64,
                self.fields.last_modified_epoch => mtime,
            ));

            count += 1;
        }

        if let Err(e) = writer.commit() {
            warn!(error = %e, "Search: Failed to commit incremental update");
        }

        debug!(file = %file.path, symbols = count, "Search: Reindexed file");
        count
    }

    /// Remove all docs for a deleted file.
    pub fn remove_file(&self, path: &str) {
        if let Ok(mut writer) = self.writer.lock() {
            let term = Term::from_field_text(self.fields.file_path, path);
            writer.delete_term(term);
            let _ = writer.commit();
            debug!(file = path, "Search: Removed file from index");
        }
    }

    /// Semantic search with per-field boosts: symbol_name (3x) > doc_comment (1.5x) > signature (1x) > body (0.5x).
    pub fn search(&self, query_str: &str, limit: usize) -> Vec<SearchResult> {
        let searcher = self.reader.searcher();

        let mut query_parser = QueryParser::for_index(
            &self.index,
            vec![
                self.fields.symbol_name,
                self.fields.doc_comment,
                self.fields.signature,
                self.fields.body_snippet,
            ],
        );

        // Per-field boosts: name matches rank highest, then docs, then signature, then body
        query_parser.set_field_boost(self.fields.symbol_name, 3.0);
        query_parser.set_field_boost(self.fields.doc_comment, 1.5);
        query_parser.set_field_boost(self.fields.signature, 1.0);
        query_parser.set_field_boost(self.fields.body_snippet, 0.5);

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

        let top_docs = match searcher.search(&query, &TopDocs::with_limit(limit)) {
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

            // Source-First (v3.9.3): boost non-test files, penalize test files
            let file_path = get_text(self.fields.file_path);
            let source_boost: f32 = if is_test_path(&file_path) { 0.5 } else { 1.5 };

            results.push(SearchResult {
                score: score * temporal_boost as f32 * source_boost,
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

        // Re-sort by temporally-adjusted score.
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
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

/// Extract the first 5 lines of a symbol's body as a snippet.
fn extract_body_snippet(source: &str, line_start: usize, line_end: usize) -> String {
    let lines: Vec<&str> = source.lines().collect();
    if line_start == 0 || line_start > lines.len() {
        return String::new();
    }

    let start = line_start.saturating_sub(1); // 0-indexed
    let end = (start + 5)
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
        let source = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8";
        let snippet = extract_body_snippet(source, 2, 7);
        assert!(snippet.starts_with("line2"));
        assert!(snippet.contains("line6"));
        assert!(!snippet.contains("line7"));
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
}
