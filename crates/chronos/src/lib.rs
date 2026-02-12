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
