//! Shadow Compiler — background `cargo check` with live diagnostics.
//!
//! Runs `cargo check --message-format=json` in the background,
//! parses compiler diagnostics, and broadcasts results on the event bus.
//! Enables the LLM to receive compilation feedback in real-time.

pub mod diagnostic;
pub mod plugin;
pub mod runner;
