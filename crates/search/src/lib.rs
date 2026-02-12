//! Semantic Code Search — Tantivy-powered full-text index over AST symbols,
//! with optional vector embedding similarity search.
//!
//! Indexes symbol names, signatures, doc comments, and body snippets
//! for concept-based search ("Where is auth logic?") rather than exact grep.

pub mod indexer;
pub mod plugin;
pub mod schema;

#[cfg(feature = "embeddings")]
pub mod embeddings;

#[cfg(feature = "embeddings")]
pub mod vector_index;
