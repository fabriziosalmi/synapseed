//! # SYNAPSEED Architect
//!
//! Structural analysis engine: builds module dependency graphs from CodeGraph
//! import symbols, calculates coupling metrics (instability, complexity),
//! detects anti-patterns (cycles, god objects, layer violations), and
//! generates architecture health reports with actionable recommendations.

pub mod analyzer;
pub mod blueprint;
pub mod linter;
pub mod plugin;

pub use analyzer::DependencyGraph;
pub use blueprint::{ArchitectureReport, ReportStore};
pub use linter::Violation;
