#![forbid(unsafe_code)]
//! # synapseed-cortex
//!
//! The Semantic Brain of SYNAPSEED. Parses source code into ASTs
//! using Tree-sitter, extracts symbol graphs, and provides semantic
//! navigation primitives (HOIST, PEEK, LOOKUP) to the MCP layer.
//!
//! The cortex never exposes raw text lines — only structured symbols.

pub mod graph;
pub(crate) mod language;
pub mod parser;
pub mod plugin;
