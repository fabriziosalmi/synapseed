#![forbid(unsafe_code)]
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
//! - `recorder` — Flight Recorder: dual-track session memory (working set + journey map)
//! - `ledger` — Cognitive Ledger: deterministic Operational Moment classification

pub mod context;
pub mod error;
pub mod event;
pub mod ledger;
pub mod liquid;
pub mod momentum;
pub mod oracle;
pub mod plugin;
pub mod policy;
pub mod pulse;
pub mod recorder;
pub mod session;
pub mod state;
pub mod symbol;
pub mod telemetry;
