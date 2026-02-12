//! # synapseed-core
//!
//! The kernel of SYNAPSEED. Defines shared types, traits, and domain
//! primitives used across all crates.
//!
//! ## Modules
//! - `symbol` — Code graph primitives (Symbol, FileStructure)
//! - `error` — Domain error types
//! - `policy` — Security policy definitions
//! - `plugin` — Plugin trait for extensible architecture
//! - `context` — Thread-safe shared state (SynapseContext)
//! - `liquid` — Dynamic configuration system (DNA)
//! - `state` — Project state detection
//! - `telemetry` — Structured logging and metrics setup
//! - `event` — Domain events for plugin communication

pub mod symbol;
pub mod error;
pub mod policy;
pub mod plugin;
pub mod context;
pub mod liquid;
pub mod state;
pub mod telemetry;
pub mod event;
