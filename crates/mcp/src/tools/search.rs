use synapseed_core::context::SynapseContext;
use synapseed_cortex::graph::CodeGraph;
use synapseed_search::indexer::{SearchResult, SemanticIndex};
use tracing::info;

use super::{error_result, text_result};
use crate::protocol::ToolCallResult;

/// Build an ephemeral Tantivy index from the project tree and search it.
fn build_ephemeral_index(
    ctx: &SynapseContext,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchResult>, String> {
    info!("Search: building ephemeral index (auto-hoist)");
    let root = ctx.project_root();
    let graph = CodeGraph::new();
    graph
        .index_directory(&root)
        .map_err(|e| format!("Failed to index project: {e}"))?;
    let index = SemanticIndex::new().map_err(|e| format!("Failed to create search index: {e}"))?;
    let files = graph.all_files();
    index.index_all(&files, &root);
    Ok(index.search(query, limit))
}

pub(super) fn tool_semantic_search(
    args: &serde_json::Value,
    ctx: &SynapseContext,
) -> ToolCallResult {
    let query = match args.get("query").and_then(|v| v.as_str()) {
        Some(q) => q,
        None => return error_result("Missing required parameter: query".into()),
    };
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

    // Try to use the persistent index from SearchPlugin.
    // If the persistent index exists but returns no results (cold start / empty),
    // fall back to building an ephemeral index on demand (auto-hoist).
    let results = if let Some(index) = ctx.get_extension::<SemanticIndex>() {
        let persistent_results = index.search(query, limit);
        if persistent_results.is_empty() {
            // Persistent index might be empty (not yet populated) — try ephemeral.
            match build_ephemeral_index(ctx, query, limit) {
                Ok(r) => r,
                Err(e) => return error_result(e),
            }
        } else {
            persistent_results
        }
    } else {
        // No persistent index at all — build ephemeral.
        match build_ephemeral_index(ctx, query, limit) {
            Ok(r) => r,
            Err(e) => return error_result(e),
        }
    };

    // D45: Warn if the index is likely still populating (cold-start).
    let cold_start_warning = if ctx
        .get_extension::<CodeGraph>()
        .is_none_or(|g| g.file_count() == 0)
    {
        "⚠ Indexing may still be in progress — results could be incomplete. Retry shortly for full coverage.\n\n"
    } else {
        ""
    };

    if results.is_empty() {
        text_result(format!(
            "{cold_start_warning}No results found for: \"{query}\""
        ))
    } else {
        let json = serde_json::to_string_pretty(&results).unwrap_or_default();
        text_result(format!(
            "{cold_start_warning}Found {} result(s) for \"{query}\":\n{json}",
            results.len()
        ))
    }
}

pub(super) fn tool_semantic_similarity(
    args: &serde_json::Value,
    ctx: &SynapseContext,
) -> ToolCallResult {
    let query = match args.get("query").and_then(|v| v.as_str()) {
        Some(q) => q.trim(),
        None => return error_result("Missing required parameter: query".into()),
    };
    if query.is_empty() {
        return error_result("Query must not be empty".into());
    }
    let top_k = args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
    let min_similarity = args
        .get("min_similarity")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.3) as f32;

    #[cfg(feature = "embeddings")]
    {
        use synapseed_search::embeddings::EmbeddingEngine;
        use synapseed_search::vector_index::VectorIndex;

        let engine = match ctx.get_extension::<EmbeddingEngine>() {
            Some(e) => e,
            None => {
                return text_result(
                    "Embeddings not available. Enable with `search.embeddings: true` in your DNA config (.synapseed/dna.yaml).".into(),
                );
            }
        };

        let vector_index = match ctx.get_extension::<VectorIndex>() {
            Some(vi) => vi,
            None => {
                return text_result(
                    "Vector index not ready. Embeddings may still be loading.".into(),
                );
            }
        };

        let query_vector = match engine.embed(query) {
            Ok(v) => v,
            Err(e) => return error_result(format!("Failed to embed query: {e}")),
        };

        let results = vector_index.search(&query_vector, top_k);
        let filtered: Vec<_> = results
            .into_iter()
            .filter(|r| r.similarity >= min_similarity)
            .collect();

        if filtered.is_empty() {
            text_result(format!(
                "No results above similarity threshold ({min_similarity}) for: \"{query}\""
            ))
        } else {
            let json = serde_json::to_string_pretty(&filtered).unwrap_or_default();
            text_result(format!(
                "Found {} similar symbol(s) for \"{query}\":\n{json}",
                filtered.len()
            ))
        }
    }

    #[cfg(not(feature = "embeddings"))]
    {
        let _ = (query, top_k, min_similarity, ctx);
        text_result(
            "Embeddings not compiled. Rebuild with the `embeddings` feature enabled.".into(),
        )
    }
}
