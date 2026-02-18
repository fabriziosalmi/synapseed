//! Hybrid Retrieval — Reciprocal Rank Fusion (v4.5.0)
//!
//! Fuses BM25 (Tantivy) and vector embedding (fastembed) search results
//! using Reciprocal Rank Fusion (RRF), a rank-based aggregation method
//! that combines results from heterogeneous retrievers without requiring
//! score normalization between them.
//!
//! ## RRF Formula
//!
//! ```text
//! RRF(d) = Σ_{r ∈ R}  1 / (k + rank_r(d))
//! ```
//!
//! Where `k = 60` (standard constant from the original paper:
//! Cormack, Clarke & Buettcher, 2009).
//!
//! Documents found by **both** retrievers get higher fused scores than those
//! found by only one, which is the key insight: BM25 captures keyword
//! relevance while vectors capture semantic similarity. The intersection
//! is the highest-quality signal.
//!
//! ## Score Normalization
//!
//! Raw RRF scores are in [0, 2/(k+1)] ≈ [0, 0.033]. We normalize to
//! [0, 1] so that downstream consumers (e.g. Whisper's `min_confidence`
//! threshold) work without recalibration.

use std::collections::HashMap;

use tracing::debug;

use crate::backend::SearchBackend;
use crate::embeddings::EmbeddingEngine;
use crate::indexer::SearchResult;
use crate::vector_index::VectorIndex;

/// Standard RRF constant from the original paper.
/// k=60 balances between giving too much weight to top-ranked results
/// (low k) and flattening the ranking (high k).
const RRF_K: f64 = 60.0;

/// Over-fetch multiplier: retrieve more candidates from each source
/// to increase the chance of finding cross-retriever matches.
const OVER_FETCH_FACTOR: usize = 3;

/// Hybrid search: fuses BM25 and vector results using Reciprocal Rank Fusion.
///
/// Returns `Vec<SearchResult>` with normalized RRF scores in [0, 1].
/// Falls back to BM25-only if embedding fails or vector index is empty.
///
/// D40: Accepts `&dyn SearchBackend` so callers are decoupled from Tantivy.
pub fn hybrid_search(
    bm25_index: &dyn SearchBackend,
    vector_index: &VectorIndex,
    embedding_engine: &EmbeddingEngine,
    query: &str,
    limit: usize,
) -> Vec<SearchResult> {
    let over_fetch = limit * OVER_FETCH_FACTOR;

    // ── BM25 branch ──────────────────────────────────────────────────
    let bm25_results = bm25_index.search(query, over_fetch);

    // ── Vector branch ────────────────────────────────────────────────
    let query_embedding = match embedding_engine.embed(query) {
        Ok(v) => v,
        Err(e) => {
            debug!(error = %e, "Hybrid RRF: embedding failed, falling back to BM25");
            return bm25_results.into_iter().take(limit).collect();
        }
    };

    let vector_results = vector_index.search(&query_embedding, over_fetch);

    if vector_results.is_empty() {
        debug!("Hybrid RRF: no vector results, returning BM25 only");
        return bm25_results.into_iter().take(limit).collect();
    }

    // ── RRF Fusion ───────────────────────────────────────────────────
    // Key: "file:symbol:line_start" to deduplicate across retrievers.
    let mut fused: HashMap<String, FusedEntry> = HashMap::new();

    let bm25_count = bm25_results.len();
    let vector_count = vector_results.len();

    // Process BM25 results (rank 0 = best)
    for (rank, result) in bm25_results.into_iter().enumerate() {
        let key = fusion_key(&result.file, &result.symbol, result.line_start);
        let rrf = 1.0 / (RRF_K + rank as f64 + 1.0);
        fused.insert(
            key,
            FusedEntry {
                rrf_score: rrf,
                result,
                sources: FusionSource::Bm25Only,
            },
        );
    }

    // Process vector results — merge with BM25 or add as vector-only
    for (rank, sim) in vector_results.into_iter().enumerate() {
        let key = fusion_key(
            &sim.entry.file_path,
            &sim.entry.symbol_name,
            sim.entry.line_start as u64,
        );
        let rrf = 1.0 / (RRF_K + rank as f64 + 1.0);

        match fused.get_mut(&key) {
            Some(entry) => {
                // Found in BOTH retrievers — the money shot
                entry.rrf_score += rrf;
                entry.sources = FusionSource::Both;
            }
            None => {
                // Vector-only: construct SearchResult from VectorEntry metadata
                fused.insert(
                    key,
                    FusedEntry {
                        rrf_score: rrf,
                        result: SearchResult {
                            score: 0.0, // placeholder, replaced by normalized RRF
                            file: sim.entry.file_path,
                            symbol: sim.entry.symbol_name,
                            kind: sim.entry.kind,
                            line_start: sim.entry.line_start as u64,
                            line_end: sim.entry.line_end as u64,
                            signature: String::new(),
                            snippet: sim.entry.embedded_text,
                            last_modified_epoch: 0,
                        },
                        sources: FusionSource::VectorOnly,
                    },
                );
            }
        }
    }

    // ── Normalize and sort ───────────────────────────────────────────
    // Max theoretical RRF: #1 in both = 2/(k+1)
    let max_rrf = 2.0 / (RRF_K + 1.0);

    let both_count = fused
        .values()
        .filter(|e| matches!(e.sources, FusionSource::Both))
        .count();

    let mut results: Vec<SearchResult> = fused
        .into_values()
        .map(|mut e| {
            e.result.score = (e.rrf_score / max_rrf) as f32;
            e.result
        })
        .collect();

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // ── Quality rerank (v5.0.1) ──────────────────────────────────────
    // Apply the same quality signals (vendor penalty, exact match bonus,
    // temporal decay, etc.) that BM25-only scoring has always applied.
    // Without this, hybrid mode ignores W_SOURCE, W_EXACT, W_TEMPORAL —
    // vendor files can dominate results and exact name matches get no boost.
    bm25_index.apply_quality_rerank(&mut results, query);

    results.truncate(limit);

    debug!(
        bm25 = bm25_count,
        vector = vector_count,
        both = both_count,
        fused = results.len(),
        "Hybrid RRF: fusion complete"
    );

    results
}

/// Unique key for deduplication across BM25 and vector results.
fn fusion_key(file: &str, symbol: &str, line_start: u64) -> String {
    format!("{file}:{symbol}:{line_start}")
}

/// Tracks which retriever(s) found this result (for logging/diagnostics).
#[derive(Debug)]
enum FusionSource {
    Bm25Only,
    VectorOnly,
    Both,
}

struct FusedEntry {
    rrf_score: f64,
    result: SearchResult,
    sources: FusionSource,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fusion_key_format() {
        let key = fusion_key("src/lib.rs", "main", 42);
        assert_eq!(key, "src/lib.rs:main:42");
    }

    #[test]
    fn test_rrf_score_normalization() {
        // #1 in both retrievers → normalized score = 1.0
        let max_rrf = 2.0 / (RRF_K + 1.0);
        let score_both_first = 2.0 * (1.0 / (RRF_K + 1.0));
        let normalized = score_both_first / max_rrf;
        assert!((normalized - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_rrf_single_source_half() {
        // #1 in one retriever only → normalized score = 0.5
        let max_rrf = 2.0 / (RRF_K + 1.0);
        let score_one_first = 1.0 / (RRF_K + 1.0);
        let normalized = score_one_first / max_rrf;
        assert!((normalized - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_rrf_both_beats_single() {
        // A result at #5 in both should beat #1 in one only
        let max_rrf = 2.0 / (RRF_K + 1.0);
        let both_rank5 = 2.0 * (1.0 / (RRF_K + 5.0 + 1.0));
        let single_rank1 = 1.0 / (RRF_K + 1.0);
        assert!(both_rank5 / max_rrf > single_rank1 / max_rrf);
    }

    #[test]
    fn test_rrf_ranking_order() {
        // Verify RRF ranks decrease with rank position
        let score_rank0 = 1.0 / (RRF_K + 1.0);
        let score_rank1 = 1.0 / (RRF_K + 2.0);
        let score_rank5 = 1.0 / (RRF_K + 6.0);
        assert!(score_rank0 > score_rank1);
        assert!(score_rank1 > score_rank5);
    }
}
