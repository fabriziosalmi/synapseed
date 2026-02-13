#![forbid(unsafe_code)]
//! # synapseed-whisper
//!
//! The Whisperer — Intent Router that orchestrates all SYNAPSEED subsystems.
//!
//! Instead of the LLM calling 4 tools in sequence, the Whisperer detects
//! intent from a natural-language query, executes the right subsystems
//! internally (pure Rust, no JSON-RPC roundtrips), and returns an enriched
//! context object in a single call.
//!
//! Level 0: Deterministic heuristic routing (keyword-based).
//! Level 1: Small LLM routing (future — plug in any classifier).

pub mod plugin;
pub mod router;
