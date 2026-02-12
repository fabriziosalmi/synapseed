//! # synapseed-husk
//!
//! The Security Shield of SYNAPSEED. Provides Data Loss Prevention
//! through ultra-fast Aho-Corasick pattern matching for static secrets
//! and regex-based detection for structured patterns (API keys, tokens).
//!
//! Every byte leaving the process passes through the husk.

pub mod guard;
pub mod plugin;
pub(crate) mod scanner;
