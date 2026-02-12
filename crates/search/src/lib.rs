//! Semantic Code Search — Tantivy-powered full-text index over AST symbols.
//!
//! Indexes symbol names, signatures, doc comments, and body snippets
//! for concept-based search ("Where is auth logic?") rather than exact grep.

pub mod indexer;
pub mod plugin;
pub mod schema;
