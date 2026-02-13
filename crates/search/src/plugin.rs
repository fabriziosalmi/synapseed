//! Search Plugin — builds and maintains the Tantivy semantic index,
//! plus optional vector embedding index for similarity search.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tracing::{debug, info, warn};

use synapseed_core::context::SynapseContext;
use synapseed_core::error::Result;
use synapseed_core::event::{FileChangeKind, SynapseEvent};
use synapseed_core::plugin::SynapsePlugin;
use synapseed_core::symbol::{FileStructure, SymbolKind};
use synapseed_cortex::graph::CodeGraph;
use synapseed_cortex::parser::AstParser;

use crate::indexer::SemanticIndex;

/// The Search plugin — semantic code search powered by Tantivy,
/// with optional vector embedding similarity search.
pub struct SearchPlugin {
    index: Option<Arc<SemanticIndex>>,
}

impl SearchPlugin {
    pub fn new() -> Self {
        Self { index: None }
    }
}

impl Default for SearchPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl SynapsePlugin for SearchPlugin {
    fn name(&self) -> &str {
        "search"
    }

    fn on_init(&mut self, ctx: &SynapseContext) -> Result<()> {
        let root = ctx.project_root();
        let dna = ctx.dna();

        // Create the semantic index (disk or RAM based on DNA config)
        let index = if dna.search.persistence {
            let index_dir = root.join(".synapseed").join("index");
            match SemanticIndex::open_or_create(&index_dir) {
                Ok(mut idx) => {
                    if let Some(lambda) = dna.search.temporal_decay_lambda {
                        idx.set_temporal_decay(lambda);
                    }
                    info!(path = %index_dir.display(), "Search: Using persistent disk index");
                    Arc::new(idx)
                }
                Err(e) => {
                    warn!(error = %e, "Search: Disk index failed, falling back to RAM");
                    match SemanticIndex::new() {
                        Ok(idx) => Arc::new(idx),
                        Err(e2) => {
                            warn!(error = %e2, "Search: Failed to create any index");
                            return Ok(());
                        }
                    }
                }
            }
        } else {
            match SemanticIndex::new() {
                Ok(mut idx) => {
                    if let Some(lambda) = dna.search.temporal_decay_lambda {
                        idx.set_temporal_decay(lambda);
                    }
                    Arc::new(idx)
                }
                Err(e) => {
                    warn!(error = %e, "Search: Failed to create Tantivy index");
                    return Ok(()); // Non-fatal — search is optional
                }
            }
        };

        // Register the index immediately so MCP tool can use it
        // (searches before indexing completes return empty results;
        //  the tool has an ephemeral fallback anyway)
        ctx.set_extension(index.clone());
        self.index = Some(index.clone());

        // Check if embeddings are enabled
        #[cfg(feature = "embeddings")]
        let embeddings_enabled = dna.search.embeddings;
        #[cfg(not(feature = "embeddings"))]
        let embeddings_enabled = false;

        // Bulk indexing runs in a background thread to avoid blocking MCP startup
        let bg_root = root.clone();
        let bg_ctx = ctx.clone();
        std::thread::spawn(move || {
            let graph = CodeGraph::new();
            if let Err(e) = graph.index_directory(&bg_root) {
                warn!(error = %e, "Search: Failed to index project for search");
                return;
            }

            if bg_ctx.is_shutting_down() {
                return;
            }

            let files = graph.all_files();
            let count = index.index_all(&files, &bg_root);
            info!(symbols = count, "Search: Semantic index ready");
            bg_ctx.broadcast(SynapseEvent::SearchReady);

            if bg_ctx.is_shutting_down() {
                return;
            }

            // Phase 2: Vector embeddings (if enabled)
            #[cfg(feature = "embeddings")]
            if embeddings_enabled {
                embed_all_symbols(&bg_root, &files, &bg_ctx);
            }
            let _ = embeddings_enabled; // suppress unused warning when feature disabled
        });

        info!("Search: Background indexing started");
        Ok(())
    }

    fn on_event<'a>(
        &'a self,
        event: &'a SynapseEvent,
        ctx: &'a SynapseContext,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SynapseEvent>>> + Send + 'a>> {
        Box::pin(async move {
            if let SynapseEvent::FileChanged { path, kind } = event {
                if let Some(index) = &self.index {
                    let root = ctx.project_root();

                    match kind {
                        FileChangeKind::Deleted => {
                            index.remove_file(path);
                            debug!(file = path, "Search: Removed deleted file from index");

                            // Also remove from vector index
                            #[cfg(feature = "embeddings")]
                            if let Some(vi) = ctx.get_extension::<crate::vector_index::VectorIndex>()
                            {
                                vi.remove_file(path);
                                debug!(file = path, "Search: Removed from vector index");
                            }
                        }
                        FileChangeKind::Created | FileChangeKind::Modified => {
                            // Re-parse and reindex the changed file
                            let file_path = std::path::Path::new(path);
                            if let Ok(mut parser) = AstParser::new() {
                                if let Ok(source) = std::fs::read_to_string(file_path) {
                                    if let Ok(file_structure) =
                                        parser.parse_file(file_path, &source)
                                    {
                                        let count = index.reindex_file(&file_structure, &root);
                                        debug!(
                                            file = path,
                                            symbols = count,
                                            "Search: Incremental reindex"
                                        );

                                        // Also update vector index
                                        #[cfg(feature = "embeddings")]
                                        reembed_file(&file_structure, ctx);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Ok(None)
        })
    }

    fn priority(&self) -> u32 {
        200 // After cortex (100) but before visualizer (250)
    }
}

/// Build the embedding text for a symbol: name + signature + doc comments, truncated.
fn build_embedding_text(sym: &synapseed_core::symbol::Symbol) -> String {
    let mut text = sym.name.clone();
    if let Some(sig) = &sym.signature {
        text.push(' ');
        text.push_str(sig);
    }
    // Truncate to 512 chars for embedding efficiency
    if text.len() > 512 {
        text.truncate(512);
    }
    text
}

/// Returns true if the symbol kind is worth embedding.
fn should_embed(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Function
            | SymbolKind::Method
            | SymbolKind::Struct
            | SymbolKind::Class
            | SymbolKind::Enum
            | SymbolKind::Interface
            | SymbolKind::Constant
    )
}

/// Bulk embed all symbols from all files into the vector index.
#[cfg(feature = "embeddings")]
fn embed_all_symbols(
    project_root: &std::path::Path,
    files: &[FileStructure],
    ctx: &SynapseContext,
) {
    use crate::embeddings::{EmbeddingConfig, EmbeddingEngine};
    use crate::vector_index::{VectorEntry, VectorIndex};

    info!("Search: Initializing embedding engine...");

    let config = EmbeddingConfig::new(project_root);
    let engine = match EmbeddingEngine::new(&config) {
        Ok(e) => Arc::new(e),
        Err(e) => {
            warn!(error = %e, "Search: Embedding engine failed to initialize");
            return;
        }
    };

    let dims = engine.dimensions();
    let vector_path = project_root
        .join(".synapseed")
        .join("embeddings")
        .join("vectors.bin");
    let vector_index = Arc::new(VectorIndex::with_persistence(dims, vector_path));

    // Register extensions so MCP tools can use them
    ctx.set_extension(engine.clone());
    ctx.set_extension(vector_index.clone());

    // Collect all embeddable symbols
    let mut texts = Vec::new();
    let mut entries = Vec::new();

    for file in files {
        for sym in &file.symbols {
            if !should_embed(sym.kind) {
                continue;
            }

            let text = build_embedding_text(sym);
            entries.push(VectorEntry {
                file_path: file.path.clone(),
                symbol_name: sym.name.clone(),
                kind: format!("{:?}", sym.kind),
                line_start: sym.line_start,
                line_end: sym.line_end,
                embedded_text: text.clone(),
            });
            texts.push(text);
        }
    }

    if texts.is_empty() {
        info!("Search: No embeddable symbols found");
        return;
    }

    info!(symbols = texts.len(), "Search: Embedding symbols...");

    // Batch embed in chunks (fastembed handles batching internally)
    let chunk_size = 256;
    let mut total_embedded = 0;

    for (chunk_texts, chunk_entries) in texts
        .chunks(chunk_size)
        .zip(entries.chunks(chunk_size))
    {
        let batch: Vec<String> = chunk_texts.to_vec();
        match engine.embed_batch(&batch) {
            Ok(vectors) => {
                vector_index.add_batch(vectors, chunk_entries.to_vec());
                total_embedded += chunk_texts.len();
            }
            Err(e) => {
                warn!(error = %e, "Search: Batch embedding failed, skipping chunk");
            }
        }
    }

    // Save to disk for persistence
    vector_index.save_to_disk();

    info!(
        embedded = total_embedded,
        files = files.len(),
        "Search: Vector index ready"
    );
}

/// Re-embed symbols for a single changed file.
#[cfg(feature = "embeddings")]
fn reembed_file(file: &FileStructure, ctx: &SynapseContext) {
    use crate::embeddings::EmbeddingEngine;
    use crate::vector_index::{VectorEntry, VectorIndex};

    let engine = match ctx.get_extension::<EmbeddingEngine>() {
        Some(e) => e,
        None => return,
    };
    let vector_index = match ctx.get_extension::<VectorIndex>() {
        Some(vi) => vi,
        None => return,
    };

    // Remove old entries for this file
    vector_index.remove_file(&file.path);

    // Collect new symbols
    let mut texts = Vec::new();
    let mut entries = Vec::new();

    for sym in &file.symbols {
        if !should_embed(sym.kind) {
            continue;
        }

        let text = build_embedding_text(sym);
        entries.push(VectorEntry {
            file_path: file.path.clone(),
            symbol_name: sym.name.clone(),
            kind: format!("{:?}", sym.kind),
            line_start: sym.line_start,
            line_end: sym.line_end,
            embedded_text: text.clone(),
        });
        texts.push(text);
    }

    if texts.is_empty() {
        return;
    }

    match engine.embed_batch(&texts) {
        Ok(vectors) => {
            vector_index.add_batch(vectors, entries);
            vector_index.save_to_disk();
            debug!(file = file.path, symbols = texts.len(), "Search: Re-embedded file");
        }
        Err(e) => {
            warn!(error = %e, file = file.path, "Search: Re-embedding failed");
        }
    }
}
