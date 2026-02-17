#![forbid(unsafe_code)]
//! # synapseed-chronos
//!
//! The Time-Travel Module of SYNAPSEED. Provides Git history analysis,
//! blame intelligence, and commit archaeology.
//!
//! Instead of just looking at code as-is, chronos answers:
//! "Why was this written this way?" by analyzing the commit history.

pub mod analyzer;
pub mod historian;
pub mod plugin;

/// Safely truncate a git OID string to 8 characters.
pub(crate) fn truncate_oid(oid: &str) -> String {
    let end = oid.len().min(8);
    oid[..end].to_string()
}
