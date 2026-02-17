#![forbid(unsafe_code)]
//! # synapseed-root
//!
//! The Infrastructure Layer of SYNAPSEED. Manages command execution
//! through a policy-driven sandbox. Commands are validated against
//! declarative JSON rules before execution.
//!
//! No command reaches the OS without passing through the sentinel.

pub mod executor;
pub mod plugin;
pub mod sentinel;
