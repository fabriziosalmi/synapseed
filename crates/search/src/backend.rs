//! Search backend trait abstraction (#74).
//!
//! Decouples the search interface from the Tantivy implementation,
//! enabling future backend swaps (SQLite FTS5, MeiliSearch, remote
//! search service) without touching consumer code.

use std::collections::HashMap;
use std::path::Path;

use synapseed_core::symbol::FileStructure;

use crate::indexer::SearchResult;

// ── Search Backend Trait ────────────────────────────────────────────────

/// Trait abstracting the search query + ranking interface.
///
/// Consumers (whisper, MCP tools) should depend on this trait rather than
/// on the concrete `SemanticIndex` type. This enables backend-agnostic
/// search queries and future backend swaps via DNA config.
///
/// # Implementors
///
/// - [`SemanticIndex`](crate::indexer::SemanticIndex) — Tantivy BM25 engine (default)
pub trait SearchBackend: Send + Sync {
    /// Run a search query and return ranked results.
    fn search(&self, query: &str, limit: usize) -> Vec<SearchResult>;

    /// Apply quality reranking heuristics to a set of results.
    fn apply_quality_rerank(&self, results: &mut [SearchResult], query: &str);

    /// Whether PageRank authority scores have been loaded.
    fn has_pagerank_scores(&self) -> bool;

    /// Inject module authority scores (file → score) for ranking boost.
    fn set_pagerank_scores(&self, scores: HashMap<String, f32>);
}

/// Trait abstracting the indexing / write interface.
///
/// Separated from [`SearchBackend`] because not all consumers need write
/// access (e.g., whisper only reads, plugins write).
pub trait IndexBackend: Send + Sync {
    /// Index all symbols from parsed file structures.
    /// Returns the number of symbols indexed.
    fn index_all(&self, files: &[FileStructure], project_root: &Path) -> usize;

    /// Re-index a single file (incremental update after edits).
    /// Returns the number of symbols indexed.
    fn reindex_file(&self, file: &FileStructure, project_root: &Path) -> usize;

    /// Remove all symbols from a file path.
    fn remove_file(&self, path: &str);

    /// Index metadata files (Cargo.toml, package.json, etc.).
    /// Returns the number of entries indexed.
    fn index_metadata_files(&self, project_root: &Path) -> usize;
}

// ── Implementation for SemanticIndex ────────────────────────────────────

impl SearchBackend for crate::indexer::SemanticIndex {
    fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        self.search(query, limit)
    }

    fn apply_quality_rerank(&self, results: &mut [SearchResult], query: &str) {
        self.apply_quality_rerank(results, query);
    }

    fn has_pagerank_scores(&self) -> bool {
        self.has_pagerank_scores()
    }

    fn set_pagerank_scores(&self, scores: HashMap<String, f32>) {
        self.set_pagerank_scores(scores);
    }
}

impl IndexBackend for crate::indexer::SemanticIndex {
    fn index_all(&self, files: &[FileStructure], project_root: &Path) -> usize {
        self.index_all(files, project_root)
    }

    fn reindex_file(&self, file: &FileStructure, project_root: &Path) -> usize {
        self.reindex_file(file, project_root)
    }

    fn remove_file(&self, path: &str) {
        self.remove_file(path);
    }

    fn index_metadata_files(&self, project_root: &Path) -> usize {
        self.index_metadata_files(project_root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify SemanticIndex implements both traits (compile-time check).
    #[test]
    fn test_semantic_index_implements_traits() {
        fn assert_search_backend<T: SearchBackend>() {}
        fn assert_index_backend<T: IndexBackend>() {}
        assert_search_backend::<crate::indexer::SemanticIndex>();
        assert_index_backend::<crate::indexer::SemanticIndex>();
    }
}
