//! Local embedding engine using fastembed (ONNX Runtime).
//!
//! Downloads and caches the model on first use.
//! Model: all-MiniLM-L6-v2 (384 dimensions, ~22MB ONNX).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use tracing::{info, warn};

/// Configuration for the embedding engine.
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    /// Directory to cache the downloaded model.
    pub cache_dir: PathBuf,
}

impl EmbeddingConfig {
    pub fn new(project_root: &Path) -> Self {
        Self {
            cache_dir: project_root.join(".synapseed").join("models"),
        }
    }
}

/// The embedding engine — wraps fastembed for local ONNX inference.
///
/// Thread-safe via interior mutability (fastembed requires `&mut self` for embed).
pub struct EmbeddingEngine {
    model: Mutex<TextEmbedding>,
    /// Dimensionality of the output vectors (384 for MiniLM-L6-v2).
    pub dimensions: usize,
}

impl std::fmt::Debug for EmbeddingEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbeddingEngine")
            .field("dimensions", &self.dimensions)
            .finish()
    }
}

impl EmbeddingEngine {
    /// Initialize the engine. Downloads model on first use (~22MB).
    ///
    /// This blocks during model download. Call from a background thread.
    pub fn new(config: &EmbeddingConfig) -> Result<Self, EmbeddingError> {
        std::fs::create_dir_all(&config.cache_dir).map_err(|e| {
            EmbeddingError::Init(format!(
                "Failed to create model cache dir {}: {e}",
                config.cache_dir.display()
            ))
        })?;

        info!(
            cache = %config.cache_dir.display(),
            "Embedding: Initializing model (downloading on first use)..."
        );

        let options = InitOptions::new(EmbeddingModel::AllMiniLML6V2)
            .with_cache_dir(config.cache_dir.clone())
            .with_show_download_progress(true);

        let model = TextEmbedding::try_new(options).map_err(|e| {
            warn!(error = %e, "Embedding: Model initialization failed");
            EmbeddingError::Init(format!("Failed to initialize embedding model: {e}"))
        })?;

        info!(
            model = "all-MiniLM-L6-v2",
            dimensions = 384,
            "Embedding: Model ready"
        );

        Ok(Self {
            model: Mutex::new(model),
            dimensions: 384,
        })
    }

    /// Embed a single text string into a vector.
    pub fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let mut model = self
            .model
            .lock()
            .map_err(|e| EmbeddingError::Inference(format!("Model lock poisoned: {e}")))?;

        let embeddings = model
            .embed(vec![text], None)
            .map_err(|e| EmbeddingError::Inference(format!("Embedding failed: {e}")))?;

        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| EmbeddingError::Inference("Empty embedding result".to_string()))
    }

    /// Embed a batch of texts. Returns vectors in the same order.
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let mut model = self
            .model
            .lock()
            .map_err(|e| EmbeddingError::Inference(format!("Model lock poisoned: {e}")))?;

        let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        model
            .embed(refs, None)
            .map_err(|e| EmbeddingError::Inference(format!("Batch embedding failed: {e}")))
    }

    /// Get the dimensionality of the embedding vectors.
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }
}

/// Errors from the embedding engine.
#[derive(Debug, thiserror::Error)]
pub enum EmbeddingError {
    #[error("Initialization error: {0}")]
    Init(String),
    #[error("Inference error: {0}")]
    Inference(String),
}
