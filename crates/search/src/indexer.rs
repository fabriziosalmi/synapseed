//! Semantic search indexer — builds and queries the Tantivy index.
//!
//! v4.11.0+: "code" tokenizer for symbol_name/signature (CamelCase + snake_case),
//!           additive normalized scoring (replaces 8x multiplicative boosts),
//!           fuzzy fallback for low-result queries.

use std::path::Path;
use std::sync::Arc;

use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, FuzzyTermQuery, Occur, QueryParser, RegexQuery};
use tantivy::schema::Value;
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};
use tracing::{debug, info, warn};

use synapseed_core::error::safe_resolve_path;
use synapseed_core::symbol::{FileStructure, Visibility};

use crate::schema::{build_schema, fields_from_schema, SearchFields, SCHEMA_VERSION};
use crate::tokenizer::register_code_tokenizer;

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
    pagerank_scores: parking_lot::RwLock<std::collections::HashMap<String, f32>>,
}

impl SemanticIndex {
    /// Create a new in-memory index.
    pub fn new() -> Result<Self, tantivy::TantivyError> {
        let (schema, fields) = build_schema();
        let index = Index::create_in_ram(schema);
        register_code_tokenizer(&index);

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

    /// Open an existing disk-based index, or create a new one.
    pub fn open_or_create(index_dir: &Path) -> Result<Self, tantivy::TantivyError> {
        let (schema, fields) = build_schema();

        if !index_dir.exists() {
            std::fs::create_dir_all(index_dir).map_err(|e| {
                tantivy::TantivyError::SystemError(format!("Failed to create index directory: {e}"))
            })?;
        }

        let (index, fields) = match Index::open_in_dir(index_dir) {
            Ok(idx) => {
                match fields_from_schema(&idx.schema()) {
                    Some(recovered) => {
                        info!("Search: Opened existing disk index");
                        (idx, recovered)
                    }
                    None => {
                        warn!("Search: Schema mismatch (v4.11+ migration), recreating disk index");
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

        register_code_tokenizer(&index);

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

    pub fn set_temporal_decay(&mut self, lambda: f64) {
        self.temporal_decay_lambda = lambda;
    }

    pub fn set_pagerank_scores(&self, scores: std::collections::HashMap<String, f32>) {
        *self.pagerank_scores.write() = scores;
    }

    pub fn has_pagerank_scores(&self) -> bool {
        !self.pagerank_scores.read().is_empty()
    }

    /// Get the PageRank score for a specific file path (D55).
    pub fn get_pagerank_score(&self, file: &str) -> Option<f32> {
        self.pagerank_scores.read().get(file).copied()
    }

    /// Index project metadata files (Cargo.toml, LICENSE, .cargo/config.toml)
    /// as searchable pseudo-documents (v4.15.0). This allows LLMs to answer
    /// questions like "what version?" or "what license?" via search.
    pub fn index_metadata_files(&self, project_root: &Path) -> usize {
        let metadata_specs: &[(&str, &str, &str)] = &[
            ("Cargo.toml", "workspace_config", "Constant"),
            ("LICENSE", "project_license", "Constant"),
            (".cargo/config.toml", "cargo_config", "Constant"),
            ("rust-toolchain.toml", "rust_toolchain", "Constant"),
        ];

        let mut writer = match self.writer.lock() {
            Ok(w) => w,
            Err(_) => return 0,
        };

        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let mut count = 0;
        for &(rel_path, symbol_name, kind) in metadata_specs {
            let abs = project_root.join(rel_path);
            if let Ok(content) = std::fs::read_to_string(&abs) {
                // Take first 80 lines as the body snippet
                let snippet: String = content.lines().take(80).collect::<Vec<_>>().join("\n");
                let doc_comment = format!("Project metadata file: {rel_path}");

                match writer.add_document(doc!(
                    self.fields.file_path => rel_path.to_string(),
                    self.fields.symbol_name => symbol_name.to_string(),
                    self.fields.kind => kind.to_string(),
                    self.fields.signature => String::new(),
                    self.fields.doc_comment => doc_comment,
                    self.fields.body_snippet => snippet,
                    self.fields.line_start => 1u64,
                    self.fields.line_end => content.lines().count() as u64,
                    self.fields.last_modified_epoch => now_secs,
                    self.fields.visibility => "public",
                    self.fields.schema_version => SCHEMA_VERSION,
                )) {
                    Ok(_) => count += 1,
                    Err(e) => warn!(error = %e, file = rel_path, "Search: Failed to index metadata file"),
                }
            }
        }

        if count > 0 {
            if let Err(e) = writer.commit() {
                warn!(error = %e, "Search: Failed to commit metadata index");
            }
            if let Err(e) = self.reader.reload() {
                warn!(error = %e, "Search: Failed to reload after metadata");
            }
            debug!(count, "Search: Indexed metadata files");
        }
        count
    }

    /// Index all symbols from a CodeGraph snapshot.
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
                    self.fields.schema_version => SCHEMA_VERSION,
                )) {
                    Ok(_) => count += 1,
                    Err(e) => warn!(error = %e, symbol = %sym.name, "Search: Failed to add document"),
                }
            }
        }

        if let Err(e) = writer.commit() {
            warn!(error = %e, "Search: Failed to commit index");
        }
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
                self.fields.schema_version => SCHEMA_VERSION,
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

    /// Semantic search with additive normalized scoring (v4.11.0).
    ///
    /// Pipeline: BM25 over-retrieve → feature extraction → min-max normalize →
    /// weighted additive scoring → fuzzy fallback if sparse → truncate.
    pub fn search(&self, query_str: &str, limit: usize) -> Vec<SearchResult> {
        let searcher = self.reader.searcher();
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

        query_parser.set_field_boost(self.fields.symbol_name, 3.0);
        query_parser.set_field_boost(self.fields.doc_comment, 2.0);
        query_parser.set_field_boost(self.fields.signature, 1.0);
        query_parser.set_field_boost(self.fields.body_snippet, 1.5);

        let query = match query_parser.parse_query(query_str) {
            Ok(q) => q,
            Err(_) => {
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

        let mut results = self.score_results(&searcher, &top_docs, query_str);

        // Prefix fallback (v4.13.0): if too few results, try prefix matching
        // on short terms (3-8 chars). "auth" → matches "authenticate", "authorization".
        if results.len() < limit / 2 {
            let prefix_results = self.prefix_search(&searcher, query_str, retrieval_limit);
            if !prefix_results.is_empty() {
                let existing: std::collections::HashSet<(String, String, u64)> = results
                    .iter()
                    .map(|r| (r.file.clone(), r.symbol.clone(), r.line_start))
                    .collect();
                for r in prefix_results {
                    if !existing.contains(&(r.file.clone(), r.symbol.clone(), r.line_start)) {
                        results.push(r);
                    }
                }
                results.sort_by(|a, b| b.score.total_cmp(&a.score));
            }
        }

        // Fuzzy fallback: if still too few results, retry with Levenshtein distance 1
        if results.len() < limit / 2 {
            let fuzzy_results = self.fuzzy_search(&searcher, query_str, retrieval_limit);
            if !fuzzy_results.is_empty() {
                let existing: std::collections::HashSet<(String, String, u64)> = results
                    .iter()
                    .map(|r| (r.file.clone(), r.symbol.clone(), r.line_start))
                    .collect();
                for r in fuzzy_results {
                    if !existing.contains(&(r.file.clone(), r.symbol.clone(), r.line_start)) {
                        results.push(r);
                    }
                }
                results.sort_by(|a, b| b.score.total_cmp(&a.score));
            }
        }

        results.truncate(limit);
        results
    }

    /// Score candidates with additive normalized features (v5.0.0).
    ///
    /// Improvements:
    /// - Pre-computed query terms outside the candidate loop (speed)
    /// - Exact name match bonus: +0.25 when a query term exactly matches symbol name (intelligence)
    fn score_results(
        &self,
        searcher: &tantivy::Searcher,
        top_docs: &[(f32, tantivy::DocAddress)],
        query_str: &str,
    ) -> Vec<SearchResult> {
        if top_docs.is_empty() {
            return Vec::new();
        }

        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        struct Candidate {
            bm25: f32,
            temporal: f32,
            source: f32,
            path_match: f32,
            specificity: f32,
            kind_boost: f32,
            pagerank: f32,
            visibility: f32,
            exact_match: f32,
            result: SearchResult,
        }

        let pr_scores = self.pagerank_scores.read();
        let mut candidates: Vec<Candidate> = Vec::with_capacity(top_docs.len());

        // Pre-compute query terms once outside the loop (v5.0.0 speed optimization).
        // Previously computed per-candidate, causing O(n * terms) lowercase allocations.
        let query_terms_lower: Vec<String> = query_str
            .split_whitespace()
            .filter(|t| t.len() >= 3)
            .map(|t| t.to_ascii_lowercase())
            .collect();

        for &(bm25_score, doc_address) in top_docs {
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
            let temporal_raw = 0.7 + 0.3 * (-self.temporal_decay_lambda * age_days).exp();
            let temporal = ((temporal_raw - 0.7) / 0.3) as f32;

            let file_path = get_text(self.fields.file_path);
            let source = if is_vendor_path(&file_path) {
                0.0
            } else if is_test_path(&file_path) {
                0.2
            } else {
                1.0
            };

            let path_lower = file_path.to_ascii_lowercase();
            let path_match = if query_terms_lower.is_empty() {
                0.0
            } else {
                query_terms_lower
                    .iter()
                    .filter(|t| path_lower.contains(&**t))
                    .count() as f32
                    / query_terms_lower.len() as f32
            };

            let symbol_name = get_text(self.fields.symbol_name);
            let specificity = ((symbol_name.len() as f32 - 4.0) / 12.0).clamp(0.0, 1.0);

            // Exact Name Match bonus (v5.0.0 — "Il Nome Esatto"):
            // When a query term exactly matches the symbol name (case-insensitive),
            // boost heavily. "ask" → should rank `ask` above `ask_with_options`.
            let sym_lower = symbol_name.to_ascii_lowercase();
            let exact_match = if query_terms_lower.iter().any(|t| *t == sym_lower) {
                1.0
            } else if query_terms_lower.iter().any(|t| sym_lower.starts_with(t.as_str())) {
                0.3 // Prefix match: weaker signal but still useful
            } else {
                0.0
            };

            let kind_value = get_text(self.fields.kind);
            let kind_boost = match kind_value.as_str() {
                "Interface" => 1.0,
                "Struct" | "Class" => 0.8,
                "Enum" => 0.6,
                "Function" | "Method" => 0.4,
                _ => 0.2,
            };

            let pagerank = pr_scores.get(&file_path).copied().unwrap_or(0.0);

            let visibility_str = get_text(self.fields.visibility);
            let is_dynamic = file_path.ends_with(".py")
                || file_path.ends_with(".js")
                || file_path.ends_with(".ts")
                || file_path.ends_with(".rb")
                || file_path.ends_with(".php");
            let visibility = if is_dynamic {
                match visibility_str.as_str() {
                    "public" => 1.0,
                    "private" => 0.7,
                    _ => 0.8,
                }
            } else {
                match visibility_str.as_str() {
                    "public" => 1.0,
                    "crate" => 0.7,
                    "super" => 0.5,
                    "private" => 0.3,
                    _ => 0.6,
                }
            };

            candidates.push(Candidate {
                bm25: bm25_score,
                temporal,
                source,
                path_match,
                specificity,
                kind_boost,
                pagerank,
                visibility,
                exact_match,
                result: SearchResult {
                    score: 0.0,
                    file: file_path,
                    symbol: symbol_name,
                    kind: kind_value,
                    line_start: get_u64(self.fields.line_start),
                    line_end: get_u64(self.fields.line_end),
                    signature: get_text(self.fields.signature),
                    snippet: get_text(self.fields.body_snippet),
                    last_modified_epoch: last_modified,
                },
            });
        }

        if candidates.is_empty() {
            return Vec::new();
        }

        // Min-max normalize BM25 across result set
        let min_bm25 = candidates.iter().map(|c| c.bm25).fold(f32::INFINITY, f32::min);
        let max_bm25 = candidates
            .iter()
            .map(|c| c.bm25)
            .fold(f32::NEG_INFINITY, f32::max);
        let bm25_range = (max_bm25 - min_bm25).max(0.001);

        // Additive scoring weights (v5.0.0: rebalanced with exact_match)
        // All features in [0, 1]. Base weights sum to 1.0, exact_match is additive bonus.
        const W_BM25: f32 = 0.40;
        const W_SOURCE: f32 = 0.12;
        const W_PATH: f32 = 0.08;
        const W_PAGERANK: f32 = 0.10;
        const W_VISIBILITY: f32 = 0.05;
        const W_KIND: f32 = 0.05;
        const W_SPECIFICITY: f32 = 0.05;
        const W_TEMPORAL: f32 = 0.05;
        const W_EXACT: f32 = 0.10;

        let mut results: Vec<SearchResult> = candidates
            .into_iter()
            .map(|mut c| {
                let norm_bm25 = (c.bm25 - min_bm25) / bm25_range;
                c.result.score = W_BM25 * norm_bm25
                    + W_SOURCE * c.source
                    + W_PATH * c.path_match
                    + W_PAGERANK * c.pagerank
                    + W_VISIBILITY * c.visibility
                    + W_KIND * c.kind_boost
                    + W_SPECIFICITY * c.specificity
                    + W_TEMPORAL * c.temporal
                    + W_EXACT * c.exact_match;
                c.result
            })
            .collect();

        results.sort_by(|a, b| b.score.total_cmp(&a.score));

        // Dedup same-symbol results (v4.15.0): when Tantivy returns duplicate
        // entries for the same (symbol, file) pair (e.g. struct impl blocks),
        // keep only the highest-scored entry to improve result diversity.
        let mut seen = std::collections::HashSet::new();
        results.retain(|r| seen.insert((r.symbol.clone(), r.file.clone())));

        results
    }

    /// Apply quality signals to pre-ranked results (v5.0.1 — RRF Quality Pass).
    ///
    /// Used by hybrid_search to apply the same quality signals (vendor penalty,
    /// exact match bonus, temporal decay, etc.) that BM25-only scoring applies
    /// but RRF was missing.  The input `results` already have an RRF-normalized
    /// score in [0, 1]; we blend quality signals at 30% weight.
    pub fn apply_quality_rerank(&self, results: &mut [SearchResult], query: &str) {
        if results.is_empty() {
            return;
        }

        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let query_terms_lower: Vec<String> = query
            .split_whitespace()
            .filter(|t| t.len() >= 3)
            .map(|t| t.to_ascii_lowercase())
            .collect();

        let pr_scores = self.pagerank_scores.read();

        for r in results.iter_mut() {
            // Source quality: vendor=0, test=0.2, source=1.0
            let source = if is_vendor_path(&r.file) {
                0.0
            } else if is_test_path(&r.file) {
                0.2
            } else {
                1.0
            };

            // Path match
            let path_lower = r.file.to_ascii_lowercase();
            let path_match = if query_terms_lower.is_empty() {
                0.0
            } else {
                query_terms_lower.iter().filter(|t| path_lower.contains(&**t)).count() as f32
                    / query_terms_lower.len() as f32
            };

            // Exact name match
            let sym_lower = r.symbol.to_ascii_lowercase();
            let exact_match = if query_terms_lower.iter().any(|t| *t == sym_lower) {
                1.0
            } else if query_terms_lower.iter().any(|t| sym_lower.starts_with(t.as_str())) {
                0.3
            } else {
                0.0
            };

            // Kind boost
            let kind_boost = match r.kind.as_str() {
                "Interface" => 1.0,
                "Struct" | "Class" => 0.8,
                "Enum" => 0.6,
                "Function" | "Method" => 0.4,
                _ => 0.2,
            };

            // Temporal decay
            let age_days = if now_secs > r.last_modified_epoch {
                (now_secs - r.last_modified_epoch) as f64 / 86400.0
            } else {
                0.0
            };
            let temporal_raw = 0.7 + 0.3 * (-self.temporal_decay_lambda * age_days).exp();
            let temporal = ((temporal_raw - 0.7) / 0.3) as f32;

            // Specificity
            let specificity = ((r.symbol.len() as f32 - 4.0) / 12.0).clamp(0.0, 1.0);

            // PageRank
            let pagerank = pr_scores.get(&r.file).copied().unwrap_or(0.0);

            // Blend: 70% RRF rank signal + 30% quality signals
            let quality = 0.12 * source
                + 0.08 * path_match
                + 0.10 * exact_match
                + 0.05 * kind_boost
                + 0.05 * temporal
                + 0.05 * specificity
                + 0.10 * pagerank;
            // quality weights sum to 0.55, normalize to [0, ~0.3]
            let quality_normalized = quality / 0.55;

            r.score = 0.70 * r.score + 0.30 * quality_normalized;
        }

        results.sort_by(|a, b| b.score.total_cmp(&a.score));
    }

    /// Fuzzy fallback: search with Levenshtein distance 1 on symbol_name.
    fn fuzzy_search(
        &self,
        searcher: &tantivy::Searcher,
        query_str: &str,
        limit: usize,
    ) -> Vec<SearchResult> {
        let terms: Vec<&str> = query_str
            .split_whitespace()
            .filter(|t| t.len() >= 3)
            .collect();

        if terms.is_empty() {
            return Vec::new();
        }

        let fuzzy_clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = terms
            .iter()
            .map(|term| {
                let t = Term::from_field_text(self.fields.symbol_name, &term.to_lowercase());
                let fuzzy = FuzzyTermQuery::new(t, 1, true);
                (Occur::Should, Box::new(fuzzy) as Box<dyn tantivy::query::Query>)
            })
            .collect();

        let combined = BooleanQuery::new(fuzzy_clauses);

        let top_docs = match searcher.search(&combined, &TopDocs::with_limit(limit)) {
            Ok(results) => results,
            Err(e) => {
                debug!(error = %e, "Search: Fuzzy fallback failed");
                return Vec::new();
            }
        };

        self.score_results(searcher, &top_docs, query_str)
    }

    /// Prefix search (v4.13.0): match terms as prefixes on symbol_name.
    /// "auth" → matches "authenticate", "authorization", "AuthMiddleware".
    /// Only used as a fallback when BM25 returns too few results.
    fn prefix_search(
        &self,
        searcher: &tantivy::Searcher,
        query_str: &str,
        limit: usize,
    ) -> Vec<SearchResult> {
        let terms: Vec<&str> = query_str
            .split_whitespace()
            .filter(|t| t.len() >= 3 && t.len() <= 12)
            .collect();

        if terms.is_empty() {
            return Vec::new();
        }

        let prefix_clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = terms
            .iter()
            .filter_map(|term| {
                let lower = term.to_lowercase();
                // Regex pattern: term.* anchored to match prefix
                let pattern = format!("{lower}.*");
                RegexQuery::from_pattern(&pattern, self.fields.symbol_name)
                    .ok()
                    .map(|q| (Occur::Should, Box::new(q) as Box<dyn tantivy::query::Query>))
            })
            .collect();

        if prefix_clauses.is_empty() {
            return Vec::new();
        }

        let combined = BooleanQuery::new(prefix_clauses);

        let top_docs = match searcher.search(&combined, &TopDocs::with_limit(limit)) {
            Ok(results) => results,
            Err(e) => {
                debug!(error = %e, "Search: Prefix fallback failed");
                return Vec::new();
            }
        };

        self.score_results(searcher, &top_docs, query_str)
    }

    fn read_source(&self, file: &FileStructure, project_root: &Path) -> Option<String> {
        let safe_path = safe_resolve_path(project_root, &file.path).ok()?;
        std::fs::read_to_string(&safe_path).ok()
    }

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

fn visibility_to_str(vis: Option<Visibility>) -> &'static str {
    match vis {
        Some(Visibility::Public) => "public",
        Some(Visibility::Crate) => "crate",
        Some(Visibility::Super) => "super",
        Some(Visibility::Private) => "private",
        None => "unknown",
    }
}

// ── Doc Comment Extraction (public for embedding enrichment) ────────

/// Extract doc comments above a symbol's start line.
pub fn extract_doc_comment(source: &str, line_start: usize) -> String {
    let lines: Vec<&str> = source.lines().collect();
    if line_start == 0 || line_start > lines.len() {
        return String::new();
    }

    let mut doc_lines = Vec::new();
    let mut i = line_start.saturating_sub(2);

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
            let cleaned = trimmed
                .trim_start_matches("/**")
                .trim_start_matches("*/")
                .trim_start_matches('*')
                .trim();
            if !cleaned.is_empty() {
                doc_lines.push(cleaned);
            }
        } else if trimmed.starts_with('#') {
            break;
        } else if trimmed.is_empty() && !doc_lines.is_empty() {
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

fn extract_body_snippet(source: &str, line_start: usize, line_end: usize) -> String {
    let lines: Vec<&str> = source.lines().collect();
    if line_start == 0 || line_start > lines.len() {
        return String::new();
    }
    let start = line_start.saturating_sub(1);
    let total_body = line_end.saturating_sub(line_start) + 1;
    let end_idx = line_end.saturating_sub(1).min(lines.len().saturating_sub(1));

    if total_body <= 40 {
        // Small function: capture all of it (up to 40 lines)
        let end = (start + 40).min(end_idx + 1).min(lines.len());
        lines[start..end].join("\n")
    } else {
        // Check for registry pattern before applying sandwich truncation.
        // Registry functions (tool lists, route tables) contain many `name: "..."` entries.
        let body = &lines[start..=(end_idx.min(lines.len() - 1))];
        let registry_header = extract_registry_summary(body);

        if let Some(header) = registry_header {
            // REGISTRY DETECTED (v4.19.1): compact format REPLACES sandwich.
            // Include signature lines (first 5) + complete item listing from the
            // registry header. This is far more useful than showing 20+20 lines
            // of code with 400 lines missing in between — the model gets ALL
            // item names in a parseable format.
            let sig_end = (start + 5).min(lines.len());
            let closing = lines[end_idx.min(lines.len() - 1)];
            let mut snippet = String::new();
            snippet.push_str(&header);
            snippet.push('\n');
            snippet.push_str(&lines[start..sig_end].join("\n"));
            snippet.push_str("\n    // ... [complete item list in REGISTRY comment above]");
            snippet.push_str("\n");
            snippet.push_str(closing.trim());
            return snippet;
        }

        // D63: Enum variant summary — for large enum bodies, extract variant names
        // so the model sees a compact inventory instead of truncated sandwich.
        let enum_header = extract_enum_variant_summary(body);
        if let Some(header) = enum_header {
            let sig_end = (start + 3).min(lines.len());
            let closing = lines[end_idx.min(lines.len() - 1)];
            let mut snippet = String::new();
            snippet.push_str(&header);
            snippet.push('\n');
            snippet.push_str(&lines[start..sig_end].join("\n"));
            snippet.push_str("\n    // ... [complete variant list in ENUM comment above]");
            snippet.push_str("\n");
            snippet.push_str(closing.trim());
            return snippet;
        }

        // Non-registry large function: sandwich strategy — first 20 + last 20 lines.
        let first_end = (start + 20).min(lines.len());
        let last_start = end_idx.saturating_sub(19).max(first_end);
        let last_end = (end_idx + 1).min(lines.len());
        let mut snippet = lines[start..first_end].join("\n");
        if last_start > first_end {
            snippet.push_str("\n// ...\n");
            snippet.push_str(&lines[last_start..last_end].join("\n"));
        }
        snippet
    }
}

/// Detect large enum bodies and return a compact summary listing ALL variant names.
///
/// Matches patterns like `VariantName,` or `VariantName { ... },` or `VariantName(...),`
/// repeated 10+ times. Returns: `// ENUM: N variants — Variant1, Variant2, ...`
fn extract_enum_variant_summary(body: &[&str]) -> Option<String> {
    let mut variants = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for line in body {
        let trimmed = line.trim();
        // Skip comments, empty lines, attributes, and the enum declaration line.
        if trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.starts_with("#[")
            || trimmed.starts_with("pub enum")
            || trimmed.starts_with("enum")
            || trimmed == "{"
            || trimmed == "}"
        {
            continue;
        }
        // Extract the variant name (first identifier before `{`, `(`, `,`, or whitespace).
        let name: String = trimmed
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty()
            && name.chars().next().map_or(false, |c| c.is_uppercase())
            && seen.insert(name.clone())
        {
            variants.push(name);
        }
    }

    if variants.len() >= 10 {
        Some(format!(
            "// ENUM: {} variants — {}",
            variants.len(),
            variants.join(", ")
        ))
    } else {
        None
    }
}

/// Detect registry-type function bodies (tool lists, route tables, etc.) and
/// return a compact summary annotation listing ALL registered item names.
///
/// Detects patterns like `name: "foo".into()`, `name: "foo"`, `"foo" =>` repeated
/// 10+ times. Returns a comment line: `// REGISTRY: N items — name1, name2, ...`
fn extract_registry_summary(body: &[&str]) -> Option<String> {
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for line in body {
        let trimmed = line.trim();
        // Match: name: "value".into() or name: "value",
        if let Some(idx) = trimmed.find("name:") {
            let after = trimmed[idx + 5..].trim_start();
            if let Some(rest) = after.strip_prefix('"') {
                if let Some(quote_end) = rest.find('"') {
                    let name = &rest[..quote_end];
                    if !name.is_empty() && seen.insert(name.to_string()) {
                        names.push(name.to_string());
                    }
                }
            }
        }
    }

    // Need at least 10 registry entries to trigger
    if names.len() < 10 {
        return None;
    }

    Some(format!(
        "// REGISTRY: {} items — {}",
        names.len(),
        names.join(", ")
    ))
}

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

/// Split a CamelCase symbol name into components (legacy, used by extraction query cleaning).
pub fn split_camel_case_for_index(name: &str) -> String {
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
        let source = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8";
        let snippet = extract_body_snippet(source, 2, 7);
        assert!(snippet.starts_with("line2"));
        assert!(snippet.contains("line7"));
        assert!(!snippet.contains("line8"));
    }

    #[test]
    fn test_empty_doc_comment() {
        let source = "fn foo() {}\n";
        let comment = extract_doc_comment(source, 1);
        assert!(comment.is_empty());
    }

    #[test]
    fn test_is_test_path() {
        assert!(is_test_path("tests/test_lowlevel.py"));
        assert!(!is_test_path("src/main.rs"));
    }

    #[test]
    fn test_additive_weights_sum_to_one() {
        let sum = 0.40_f32 + 0.12 + 0.08 + 0.10 + 0.05 + 0.05 + 0.05 + 0.05 + 0.10;
        assert!((sum - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_extract_registry_summary_detects_tool_list() {
        // Simulate a registry function with 12 name: "..." entries
        let mut lines = vec!["pub fn list_tools() -> Vec<ToolDefinition> {", "    vec!["];
        let tool_names: Vec<String> = (1..=12)
            .map(|i| format!("        ToolDefinition {{ name: \"tool{}\".into(), .. }}", i))
            .collect();
        let tool_lines: Vec<&str> = tool_names.iter().map(|s| s.as_str()).collect();
        lines.extend_from_slice(&tool_lines);
        lines.push("    ]");
        lines.push("}");

        let summary = extract_registry_summary(&lines);
        assert!(summary.is_some(), "Should detect registry pattern with 12 entries");
        let s = summary.unwrap();
        assert!(s.contains("REGISTRY: 12 items"), "Should report 12 items, got: {s}");
        for i in 1..=12 {
            assert!(s.contains(&format!("tool{i}")), "Should list tool{i}");
        }
    }

    #[test]
    fn test_extract_registry_summary_ignores_small_lists() {
        let lines = vec![
            "pub fn small_fn() {",
            "    name: \"a\".into(),",
            "    name: \"b\".into(),",
            "}",
        ];
        assert!(extract_registry_summary(&lines).is_none(), "Should not trigger for <10 entries");
    }

    #[test]
    fn test_extract_body_snippet_with_registry_compact() {
        // Build a >40 line function with 15 name: entries
        let mut source_lines = vec!["header".to_string(), "pub fn list_tools() -> Vec<X> {".to_string()];
        for i in 1..=15 {
            source_lines.push(format!("    Item {{ name: \"item{i}\".into() }},"));
            source_lines.push("    // filler line".to_string());
            source_lines.push("    // more filler".to_string());
        }
        source_lines.push("}".to_string());
        let source = source_lines.join("\n");

        // line_start=2, line_end = total lines
        let snippet = extract_body_snippet(&source, 2, source_lines.len());
        assert!(snippet.contains("REGISTRY: 15 items"), "Snippet should contain registry summary, got: {snippet}");
        // Compact format: all items listed in the REGISTRY header
        assert!(snippet.contains("item1"));
        assert!(snippet.contains("item15"));
        // Should NOT contain the sandwich separator — compact format replaces it
        assert!(!snippet.contains("// ...\n"), "Registry snippet should use compact format, not sandwich. Got: {snippet}");
        // Should contain the closing brace
        assert!(snippet.contains("}"), "Snippet should include closing delimiter");
    }

    #[test]
    fn test_extract_body_snippet_sandwich_for_non_registry() {
        // Build a >40 line function WITHOUT registry pattern
        let mut source_lines = vec!["pub fn big_function() {".to_string()];
        for i in 1..=50 {
            source_lines.push(format!("    let x{i} = {i};"));
        }
        source_lines.push("}".to_string());
        let source = source_lines.join("\n");

        let snippet = extract_body_snippet(&source, 1, source_lines.len());
        // Non-registry should still use sandwich
        assert!(snippet.contains("// ..."), "Non-registry large function should use sandwich, got: {snippet}");
        assert!(!snippet.contains("REGISTRY"), "No registry header for non-registry function");
    }
}
