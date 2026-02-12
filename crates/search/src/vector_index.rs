//! Vector similarity index — stores embeddings and supports kNN search.
//!
//! V1: Brute-force cosine similarity (fast enough for <10k vectors).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

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

/// In-memory vector index with brute-force cosine similarity search.
pub struct VectorIndex {
    vectors: RwLock<Vec<Vec<f32>>>,
    entries: RwLock<Vec<VectorEntry>>,
    file_map: RwLock<HashMap<String, Vec<usize>>>,
    dimensions: usize,
    storage_path: Option<PathBuf>,
}

impl std::fmt::Debug for VectorIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VectorIndex")
            .field("dimensions", &self.dimensions)
            .field("count", &self.vectors.read().map(|v| v.len()).unwrap_or(0))
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
        };
        index.load_from_disk();
        index
    }

    /// Add a batch of vectors with their metadata.
    pub fn add_batch(&self, vectors: Vec<Vec<f32>>, entries: Vec<VectorEntry>) {
        let mut vecs = self.vectors.write().unwrap();
        let mut ents = self.entries.write().unwrap();
        let mut file_map = self.file_map.write().unwrap();

        for (vector, entry) in vectors.into_iter().zip(entries.into_iter()) {
            debug_assert_eq!(vector.len(), self.dimensions);
            let idx = vecs.len();
            let file_path = entry.file_path.clone();
            vecs.push(vector);
            ents.push(entry);
            file_map.entry(file_path).or_default().push(idx);
        }
    }

    /// Remove all vectors for a given file path (mark as deleted).
    pub fn remove_file(&self, file_path: &str) {
        let mut file_map = self.file_map.write().unwrap();
        if let Some(indices) = file_map.remove(file_path) {
            let mut entries = self.entries.write().unwrap();
            for idx in indices {
                if idx < entries.len() {
                    entries[idx].file_path.clear(); // mark as deleted
                }
            }
        }
    }

    /// Search for the top-k most similar vectors to the query.
    pub fn search(&self, query: &[f32], top_k: usize) -> Vec<SimilarityResult> {
        debug_assert_eq!(query.len(), self.dimensions);

        let vectors = self.vectors.read().unwrap();
        let entries = self.entries.read().unwrap();

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

    /// Number of active (non-deleted) vectors.
    pub fn active_count(&self) -> usize {
        self.entries
            .read()
            .unwrap()
            .iter()
            .filter(|e| !e.file_path.is_empty())
            .count()
    }

    /// Number of unique files indexed.
    pub fn file_count(&self) -> usize {
        self.file_map.read().unwrap().len()
    }

    /// Save the index to disk (compacts deleted entries).
    pub fn save_to_disk(&self) {
        let Some(ref path) = self.storage_path else {
            return;
        };

        let vectors = self.vectors.read().unwrap();
        let entries = self.entries.read().unwrap();

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

                        let mut vectors = self.vectors.write().unwrap();
                        let mut entries = self.entries.write().unwrap();
                        let mut file_map = self.file_map.write().unwrap();

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
}
