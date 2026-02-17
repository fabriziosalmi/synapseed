//! Vector similarity index — stores embeddings and supports kNN search.
//!
//! V1: Brute-force cosine similarity (fast enough for <10k vectors).
//! V2: Hybrid — brute-force for small indices, HNSW (via `hnsw_rs`)
//!     for large indices (>= ANN_THRESHOLD vectors).  Activated when the
//!     `embeddings` feature is enabled (which gates `hnsw_rs`).
//! V3: **Incremental ANN** — new vectors are inserted into the existing
//!     HNSW graph without full rebuild.  Deletions tombstone entries;
//!     `compact()` performs a full rebuild to reclaim space.
//!
//! Performance characteristics:
//! - `add_batch()`:  O(k · log n) incremental insert (k = batch size)
//! - `remove_file()`: O(k) tombstone (no rebuild)
//! - `search()`:     O(log n · d) ANN or O(n · d) brute-force
//! - `compact()`:    O(n · log n) full rebuild (call when tombstone ratio > 20%)

use std::collections::HashMap;
use std::path::PathBuf;

use parking_lot::RwLock;

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Threshold above which the HNSW graph is built for ANN search.
/// Below this, brute-force linear scan is used (lower overhead).
const ANN_THRESHOLD: usize = 1000;

/// Metadata associated with each embedding vector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorEntry {
    /// File path containing the symbol.
    pub file_path: String,
    /// Symbol name.
    pub symbol_name: String,
    /// Symbol kind (Function, Struct, etc.).
    pub kind: String,
    /// Start line in the file.
    pub line_start: usize,
    /// End line in the file.
    pub line_end: usize,
    /// The text that was embedded (truncated).
    pub embedded_text: String,
}

/// Result from a similarity search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarityResult {
    /// Cosine similarity score [0.0, 1.0].
    pub similarity: f32,
    /// The matching entry's metadata.
    pub entry: VectorEntry,
}

/// Statistics about the vector index health.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorIndexStats {
    /// Total entries (including tombstoned).
    pub total_entries: usize,
    /// Active (non-tombstoned) entries.
    pub active_entries: usize,
    /// Tombstoned entries awaiting compaction.
    pub tombstoned_entries: usize,
    /// Vector dimensionality.
    pub dimensions: usize,
    /// Number of distinct files indexed.
    pub file_count: usize,
    /// Whether ANN (HNSW) is active.
    pub ann_active: bool,
    /// Threshold for ANN activation.
    pub ann_threshold: usize,
}

impl VectorIndexStats {
    /// Ratio of tombstoned entries to total (0.0–1.0).
    pub fn tombstone_ratio(&self) -> f64 {
        if self.total_entries == 0 {
            0.0
        } else {
            self.tombstoned_entries as f64 / self.total_entries as f64
        }
    }
}

/// HNSW graph wrapper — only compiled when embeddings feature is active.
#[cfg(feature = "embeddings")]
use hnsw_rs::prelude::*;

#[cfg(feature = "embeddings")]
struct HnswGraph {
    hnsw: Hnsw<'static, f32, DistCosine>,
}

/// In-memory vector index with hybrid brute-force / HNSW search.
///
/// Below [`ANN_THRESHOLD`] vectors: O(n·d) brute-force cosine scan.
/// Above threshold: O(log n · d) HNSW approximate nearest neighbor.
pub struct VectorIndex {
    vectors: RwLock<Vec<Vec<f32>>>,
    entries: RwLock<Vec<VectorEntry>>,
    file_map: RwLock<HashMap<String, Vec<usize>>>,
    dimensions: usize,
    storage_path: Option<PathBuf>,
    /// HNSW graph — rebuilt when active vector count crosses ANN_THRESHOLD.
    #[cfg(feature = "embeddings")]
    hnsw: RwLock<Option<HnswGraph>>,
}

impl std::fmt::Debug for VectorIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.vectors.read().len();
        let _ann_active;
        #[cfg(feature = "embeddings")]
        {
            _ann_active = self.hnsw.read().is_some();
        }
        #[cfg(not(feature = "embeddings"))]
        {
            _ann_active = false;
        }
        f.debug_struct("VectorIndex")
            .field("dimensions", &self.dimensions)
            .field("count", &count)
            .field("ann_active", &_ann_active)
            .finish()
    }
}

impl VectorIndex {
    /// Create a new in-memory vector index.
    pub fn new(dimensions: usize) -> Self {
        Self {
            vectors: RwLock::new(Vec::new()),
            entries: RwLock::new(Vec::new()),
            file_map: RwLock::new(HashMap::new()),
            dimensions,
            storage_path: None,
            #[cfg(feature = "embeddings")]
            hnsw: RwLock::new(None),
        }
    }

    /// Create a persistent vector index that saves/loads from disk.
    pub fn with_persistence(dimensions: usize, path: PathBuf) -> Self {
        let index = Self {
            vectors: RwLock::new(Vec::new()),
            entries: RwLock::new(Vec::new()),
            file_map: RwLock::new(HashMap::new()),
            dimensions,
            storage_path: Some(path),
            #[cfg(feature = "embeddings")]
            hnsw: RwLock::new(None),
        };
        index.load_from_disk();
        index.maybe_update_hnsw(0);
        index
    }

    /// Add a batch of vectors with their metadata.
    ///
    /// If the HNSW graph exists, new vectors are inserted incrementally
    /// (O(k·log n) instead of O(n·log n) full rebuild).
    pub fn add_batch(&self, vectors: Vec<Vec<f32>>, entries: Vec<VectorEntry>) {
        let start_idx;
        {
            let mut vecs = self.vectors.write();
            let mut ents = self.entries.write();
            let mut file_map = self.file_map.write();

            start_idx = vecs.len();

            for (vector, entry) in vectors.into_iter().zip(entries.into_iter()) {
                debug_assert_eq!(vector.len(), self.dimensions);
                let idx = vecs.len();
                let file_path = entry.file_path.clone();
                vecs.push(vector);
                ents.push(entry);
                file_map.entry(file_path).or_default().push(idx);
            }
        }
        // Locks released — update HNSW incrementally
        self.maybe_update_hnsw(start_idx);
    }

    /// Remove all vectors for a given file path (tombstone, no rebuild).
    ///
    /// Tombstoned entries are filtered at search time.  Call `compact()`
    /// when `stats().tombstone_ratio()` exceeds 0.2 to reclaim memory.
    pub fn remove_file(&self, file_path: &str) {
        let mut file_map = self.file_map.write();
        if let Some(indices) = file_map.remove(file_path) {
            let mut entries = self.entries.write();
            for idx in indices {
                if idx < entries.len() {
                    entries[idx].file_path.clear(); // tombstone
                }
            }
            // Note: HNSW NOT rebuilt — tombstoned entries are filtered at
            // search time.  Use compact() to reclaim space periodically.
        }
    }

    /// Search for the top-k most similar vectors to the query.
    ///
    /// Uses HNSW when available and vector count >= ANN_THRESHOLD,
    /// otherwise falls back to brute-force cosine scan.
    pub fn search(&self, query: &[f32], top_k: usize) -> Vec<SimilarityResult> {
        debug_assert_eq!(query.len(), self.dimensions);

        #[cfg(feature = "embeddings")]
        {
            if let Some(results) = self.search_hnsw(query, top_k) {
                return results;
            }
        }

        self.search_brute_force(query, top_k)
    }

    /// Brute-force linear scan — O(n·d).
    fn search_brute_force(&self, query: &[f32], top_k: usize) -> Vec<SimilarityResult> {
        let vectors = self.vectors.read();
        let entries = self.entries.read();

        let mut scored: Vec<(f32, usize)> = vectors
            .iter()
            .enumerate()
            .filter(|(i, _)| entries.get(*i).is_some_and(|e| !e.file_path.is_empty()))
            .map(|(i, vec)| (cosine_similarity(query, vec), i))
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        scored
            .into_iter()
            .take(top_k)
            .filter(|(sim, _)| *sim > 0.0)
            .map(|(similarity, idx)| SimilarityResult {
                similarity,
                entry: entries[idx].clone(),
            })
            .collect()
    }

    /// HNSW approximate nearest neighbor search.
    /// Returns None if HNSW graph is not built (below threshold).
    #[cfg(feature = "embeddings")]
    fn search_hnsw(&self, query: &[f32], top_k: usize) -> Option<Vec<SimilarityResult>> {
        let graph_guard = self.hnsw.read();
        let graph = graph_guard.as_ref()?;

        let entries = self.entries.read();

        // HNSW ef_search — oversample 2x to compensate for deleted entries
        let ef_search = (top_k * 2).max(32);
        let neighbours = graph.hnsw.search(query, top_k.max(ef_search), ef_search);

        let mut results: Vec<SimilarityResult> = neighbours
            .into_iter()
            .filter_map(|n| {
                let idx = n.d_id;
                let entry = entries.get(idx)?;
                if entry.file_path.is_empty() {
                    return None; // tombstoned
                }
                // hnsw_rs DistCosine returns distance = 1 - cosine_similarity
                let similarity = 1.0 - n.distance;
                if similarity > 0.0 {
                    Some(SimilarityResult {
                        similarity,
                        entry: entry.clone(),
                    })
                } else {
                    None
                }
            })
            .collect();

        results.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(top_k);

        Some(results)
    }

    /// Update HNSW after `add_batch`: incremental insert when graph exists,
    /// full build when crossing the threshold for the first time.
    fn maybe_update_hnsw(&self, new_start_idx: usize) {
        #[cfg(feature = "embeddings")]
        {
            let vectors = self.vectors.read();
            let entries = self.entries.read();
            let active_count = entries.iter().filter(|e| !e.file_path.is_empty()).count();

            if active_count < ANN_THRESHOLD {
                drop(vectors);
                drop(entries);
                let mut graph = self.hnsw.write();
                if graph.is_some() {
                    debug!(active_count, "Below ANN threshold, dropping HNSW graph");
                    *graph = None;
                }
                return;
            }

            let mut graph = self.hnsw.write();
            if let Some(ref g) = *graph {
                // ── Incremental insert: O(k·log n) ────────────────────
                let mut inserted = 0usize;
                for idx in new_start_idx..vectors.len() {
                    if entries.get(idx).is_some_and(|e| !e.file_path.is_empty()) {
                        g.hnsw.insert((&vectors[idx], idx));
                        inserted += 1;
                    }
                }
                debug!(
                    inserted,
                    total_active = active_count,
                    "HNSW: incremental insert"
                );
            } else {
                // ── First build above threshold: O(n·log n) ───────────
                let nb_layer = ((active_count as f64).log2().ceil() as usize).max(4);
                let max_nb_connection = 24;
                let ef_construction = 200;

                let hnsw = Hnsw::<f32, DistCosine>::new(
                    max_nb_connection,
                    active_count,
                    nb_layer,
                    ef_construction,
                    DistCosine {},
                );

                for (i, v) in vectors.iter().enumerate() {
                    if entries.get(i).is_some_and(|e| !e.file_path.is_empty()) {
                        hnsw.insert((v, i));
                    }
                }

                *graph = Some(HnswGraph { hnsw });
                info!(
                    vectors = active_count,
                    layers = nb_layer,
                    "HNSW graph built (initial)"
                );
            }
        }

        #[cfg(not(feature = "embeddings"))]
        {
            let _ = new_start_idx;
        }
    }

    /// Force a full HNSW rebuild (used after `compact()`).
    fn rebuild_hnsw_full(&self) {
        #[cfg(feature = "embeddings")]
        {
            *self.hnsw.write() = None;
            self.maybe_update_hnsw(0);
        }
    }

    /// Whether the HNSW graph is currently active.
    pub fn is_ann_active(&self) -> bool {
        #[cfg(feature = "embeddings")]
        {
            self.hnsw.read().is_some()
        }
        #[cfg(not(feature = "embeddings"))]
        {
            false
        }
    }

    /// Number of active (non-deleted) vectors.
    pub fn active_count(&self) -> usize {
        self.entries
            .read()
            .iter()
            .filter(|e| !e.file_path.is_empty())
            .count()
    }

    /// Number of unique files indexed.
    pub fn file_count(&self) -> usize {
        self.file_map.read().len()
    }

    /// Compact the index by removing tombstoned entries and rebuilding HNSW.
    ///
    /// Call when `stats().tombstone_ratio()` exceeds ~0.2 to reclaim memory
    /// and improve HNSW recall (tombstoned entries waste graph capacity).
    pub fn compact(&self) {
        let mut vecs = self.vectors.write();
        let mut ents = self.entries.write();
        let mut file_map = self.file_map.write();

        let total = ents.len();
        let active: Vec<(Vec<f32>, VectorEntry)> = vecs
            .iter()
            .zip(ents.iter())
            .filter(|(_, e)| !e.file_path.is_empty())
            .map(|(v, e)| (v.clone(), e.clone()))
            .collect();

        let tombstones = total - active.len();
        if tombstones == 0 {
            return;
        }

        vecs.clear();
        ents.clear();
        file_map.clear();

        for (vec, entry) in active {
            let idx = vecs.len();
            let fp = entry.file_path.clone();
            vecs.push(vec);
            ents.push(entry);
            file_map.entry(fp).or_default().push(idx);
        }

        info!(
            compacted = vecs.len(),
            tombstones_removed = tombstones,
            "Vector index compacted"
        );

        drop(vecs);
        drop(ents);
        drop(file_map);
        self.rebuild_hnsw_full();
    }

    /// Get statistics about the vector index.
    pub fn stats(&self) -> VectorIndexStats {
        let entries = self.entries.read();
        let total = entries.len();
        let active = entries.iter().filter(|e| !e.file_path.is_empty()).count();
        drop(entries);

        VectorIndexStats {
            total_entries: total,
            active_entries: active,
            tombstoned_entries: total - active,
            dimensions: self.dimensions,
            file_count: self.file_map.read().len(),
            ann_active: self.is_ann_active(),
            ann_threshold: ANN_THRESHOLD,
        }
    }

    /// Save the index to disk (compacts deleted entries).
    pub fn save_to_disk(&self) {
        let Some(ref path) = self.storage_path else {
            return;
        };

        let vectors = self.vectors.read();
        let entries = self.entries.read();

        let active: Vec<(Vec<f32>, VectorEntry)> = vectors
            .iter()
            .zip(entries.iter())
            .filter(|(_, e)| !e.file_path.is_empty())
            .map(|(v, e)| (v.clone(), e.clone()))
            .collect();

        let data = StoredIndex {
            dimensions: self.dimensions,
            vectors: active.iter().map(|(v, _)| v.clone()).collect(),
            entries: active.iter().map(|(_, e)| e.clone()).collect(),
        };

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        match bincode::serialize(&data) {
            Ok(bytes) => {
                if let Err(e) = std::fs::write(path, &bytes) {
                    warn!(error = %e, "Failed to save vector index");
                } else {
                    debug!(vectors = data.vectors.len(), bytes = bytes.len(), "Vector index saved to disk (bincode)");
                }
            }
            Err(e) => warn!(error = %e, "Failed to serialize vector index"),
        }
    }

    /// Load the index from disk.
    fn load_from_disk(&self) {
        let Some(ref path) = self.storage_path else {
            return;
        };

        if !path.exists() {
            return;
        }

        match std::fs::read(path) {
            Ok(bytes) => {
                // Try bincode first (new format), fall back to JSON (legacy)
                let parsed = bincode::deserialize::<StoredIndex>(&bytes)
                    .or_else(|_| serde_json::from_slice::<StoredIndex>(&bytes));
                match parsed {
                    Ok(data) => {
                        if data.dimensions != self.dimensions {
                            warn!(
                                stored = data.dimensions,
                                expected = self.dimensions,
                                "Dimension mismatch, discarding stored vector index"
                            );
                            return;
                        }

                        let mut vectors = self.vectors.write();
                        let mut entries = self.entries.write();
                        let mut file_map = self.file_map.write();

                        for (vec, entry) in
                            data.vectors.into_iter().zip(data.entries.into_iter())
                        {
                            let idx = vectors.len();
                            let fp = entry.file_path.clone();
                            vectors.push(vec);
                            entries.push(entry);
                            file_map.entry(fp).or_default().push(idx);
                        }

                        info!(vectors = vectors.len(), "Vector index loaded from disk");
                    }
                    Err(e) => warn!(error = %e, "Failed to deserialize vector index"),
                }
            }
            Err(e) => warn!(error = %e, "Failed to read vector index file"),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct StoredIndex {
    dimensions: usize,
    vectors: Vec<Vec<f32>>,
    entries: Vec<VectorEntry>,
}

/// Compute cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn test_vector_index_add_and_search() {
        let index = VectorIndex::new(3);
        index.add_batch(
            vec![vec![1.0, 0.0, 0.0], vec![0.9, 0.1, 0.0]],
            vec![
                VectorEntry {
                    file_path: "test.rs".into(),
                    symbol_name: "foo".into(),
                    kind: "Function".into(),
                    line_start: 1,
                    line_end: 10,
                    embedded_text: "fn foo()".into(),
                },
                VectorEntry {
                    file_path: "test.rs".into(),
                    symbol_name: "bar".into(),
                    kind: "Function".into(),
                    line_start: 11,
                    line_end: 20,
                    embedded_text: "fn bar()".into(),
                },
            ],
        );

        let results = index.search(&[1.0, 0.0, 0.0], 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].entry.symbol_name, "foo");
        assert!(results[0].similarity > results[1].similarity);
    }

    #[test]
    fn test_remove_file() {
        let index = VectorIndex::new(3);
        index.add_batch(
            vec![vec![1.0, 0.0, 0.0]],
            vec![VectorEntry {
                file_path: "test.rs".into(),
                symbol_name: "foo".into(),
                kind: "Function".into(),
                line_start: 1,
                line_end: 10,
                embedded_text: "fn foo()".into(),
            }],
        );

        assert_eq!(index.active_count(), 1);
        index.remove_file("test.rs");
        assert_eq!(index.active_count(), 0);
        assert!(index.search(&[1.0, 0.0, 0.0], 1).is_empty());
    }

    #[test]
    fn test_ann_not_active_below_threshold() {
        let index = VectorIndex::new(3);
        // Add just a few vectors — well below ANN_THRESHOLD
        index.add_batch(
            vec![vec![1.0, 0.0, 0.0]],
            vec![VectorEntry {
                file_path: "test.rs".into(),
                symbol_name: "foo".into(),
                kind: "Function".into(),
                line_start: 1,
                line_end: 10,
                embedded_text: "fn foo()".into(),
            }],
        );
        // Should use brute-force, not ANN
        assert!(!index.is_ann_active());
    }

    #[cfg(feature = "embeddings")]
    #[test]
    fn test_ann_activates_above_threshold() {
        let dims = 8;
        let index = VectorIndex::new(dims);

        // Generate ANN_THRESHOLD + 100 random-ish vectors
        let n = ANN_THRESHOLD + 100;
        let mut vectors = Vec::with_capacity(n);
        let mut entries = Vec::with_capacity(n);
        for i in 0..n {
            // Deterministic pseudo-random via simple hash mixing
            let v: Vec<f32> = (0..dims)
                .map(|d| {
                    let seed = (i * 31 + d * 7) as f32;
                    (seed.sin() * 100.0).fract().abs()
                })
                .collect();
            vectors.push(v);
            entries.push(VectorEntry {
                file_path: format!("file_{i}.rs"),
                symbol_name: format!("sym_{i}"),
                kind: "Function".into(),
                line_start: i,
                line_end: i + 10,
                embedded_text: format!("fn sym_{i}()"),
            });
        }

        index.add_batch(vectors.clone(), entries);
        assert!(index.is_ann_active(), "HNSW should be active above threshold");

        // Search should return results
        let query: Vec<f32> = (0..dims).map(|d| (d as f32 * 0.1).sin()).collect();
        let results = index.search(&query, 10);
        assert!(!results.is_empty(), "ANN search should return results");
        assert!(results.len() <= 10);

        // Results should be sorted by similarity descending
        for w in results.windows(2) {
            assert!(
                w[0].similarity >= w[1].similarity,
                "results must be sorted descending"
            );
        }

        // Recall check: compare ANN results against brute-force
        let exact = index.search_brute_force(&query, 10);
        let exact_names: std::collections::HashSet<_> =
            exact.iter().map(|r| &r.entry.symbol_name).collect();
        let ann_names: std::collections::HashSet<_> =
            results.iter().map(|r| &r.entry.symbol_name).collect();
        let recall = ann_names.intersection(&exact_names).count() as f64 / exact.len() as f64;
        assert!(
            recall >= 0.8,
            "Recall@10 should be >= 0.80 (got {recall:.2})"
        );
    }
}
