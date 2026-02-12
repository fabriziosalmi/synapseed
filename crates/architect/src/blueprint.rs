//! Architecture health report generator.
//!
//! Aggregates metrics and violations into a scored report with
//! actionable recommendations.

use parking_lot::RwLock;

use serde::{Deserialize, Serialize};

use crate::analyzer::{DependencyGraph, ModuleMetrics};
use crate::linter::{Violation, ViolationSeverity};

/// The full architecture health report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureReport {
    /// Overall health score [0, 100]. 100 = perfect architecture.
    pub score: u32,
    /// Grade label: A (90-100), B (75-89), C (60-74), D (40-59), F (0-39).
    pub grade: String,
    /// Total modules analyzed.
    pub module_count: usize,
    /// Total dependency edges.
    pub edge_count: usize,
    /// Average instability across all modules.
    pub avg_instability: f64,
    /// Average complexity across all modules.
    pub avg_complexity: f64,
    /// Maximum coupling weight between any two modules.
    pub max_coupling: usize,
    /// Topological density: D = E / (V × (V − 1)). Range [0.0, 1.0].
    pub topological_density: f64,
    /// Per-module metrics.
    pub modules: Vec<ModuleMetrics>,
    /// All detected violations.
    pub violations: Vec<Violation>,
    /// High-level actionable recommendations.
    pub recommendations: Vec<Recommendation>,
}

/// A concrete architectural recommendation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// Priority: 1 = most urgent.
    pub priority: u32,
    /// Category: "decouple", "split", "restructure".
    pub category: String,
    /// Human-readable action description.
    pub action: String,
    /// Modules involved.
    pub modules: Vec<String>,
}

/// Thread-safe store for the most recent architecture report.
pub struct ReportStore {
    report: RwLock<Option<ArchitectureReport>>,
}

impl std::fmt::Debug for ReportStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let score = self.report.read().as_ref().map(|r| r.score);
        f.debug_struct("ReportStore")
            .field("score", &score)
            .finish()
    }
}

impl ReportStore {
    pub fn new() -> Self {
        Self {
            report: RwLock::new(None),
        }
    }

    pub fn set(&self, report: ArchitectureReport) {
        *self.report.write() = Some(report);
    }

    pub fn get(&self) -> Option<ArchitectureReport> {
        self.report.read().clone()
    }

    /// Quick health check: returns (score, violation_count).
    pub fn health(&self) -> Option<(u32, usize)> {
        let guard = self.report.read();
        guard.as_ref().map(|r| (r.score, r.violations.len()))
    }
}

impl Default for ReportStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate the full architecture report from the dependency graph and violations.
pub fn generate_report(
    dep_graph: &DependencyGraph,
    violations: Vec<Violation>,
) -> ArchitectureReport {
    let metrics = dep_graph.all_metrics();

    let avg_instability = if metrics.is_empty() {
        0.0
    } else {
        metrics.iter().map(|m| m.instability).sum::<f64>() / metrics.len() as f64
    };

    let avg_complexity = if metrics.is_empty() {
        0.0
    } else {
        metrics
            .iter()
            .map(|m| m.approx_complexity as f64)
            .sum::<f64>()
            / metrics.len() as f64
    };

    let max_coupling = dep_graph
        .all_edges()
        .iter()
        .map(|(_, _, e)| e.weight)
        .max()
        .unwrap_or(0);

    let topological_density = dep_graph.topological_density();

    let score = calculate_score(
        &violations,
        avg_instability,
        avg_complexity,
        max_coupling,
        topological_density,
    );
    let grade = score_to_grade(score);
    let recommendations = generate_recommendations(dep_graph, &violations, metrics);

    ArchitectureReport {
        score,
        grade,
        module_count: dep_graph.module_count(),
        edge_count: dep_graph.edge_count(),
        avg_instability,
        avg_complexity,
        max_coupling,
        topological_density,
        modules: metrics.to_vec(),
        violations,
        recommendations,
    }
}

/// Calculate the health score [0, 100].
///
/// - Start at 100
/// - -15 per Critical violation
/// - -8 per Error violation (cycles, layer violations)
/// - -3 per Warning violation (god objects)
/// - -5 if avg_instability > 0.7
/// - -5 if avg_complexity > 20
/// - -5 if max_coupling > 15
/// - Floor at 0
fn calculate_score(
    violations: &[Violation],
    avg_instability: f64,
    avg_complexity: f64,
    max_coupling: usize,
    topological_density: f64,
) -> u32 {
    let mut score: i32 = 100;

    for v in violations {
        match v.severity {
            ViolationSeverity::Critical => score -= 15,
            ViolationSeverity::Error => score -= 8,
            ViolationSeverity::Warning => score -= 3,
        }
    }

    if avg_instability > 0.7 {
        score -= 5;
    }
    if avg_complexity > 20.0 {
        score -= 5;
    }
    if max_coupling > 15 {
        score -= 5;
    }
    // Density penalty: over-connected graph is hard to evolve.
    if topological_density > 0.5 {
        score -= 5;
    }

    score.max(0) as u32
}

fn score_to_grade(score: u32) -> String {
    match score {
        90..=100 => "A".to_string(),
        75..=89 => "B".to_string(),
        60..=74 => "C".to_string(),
        40..=59 => "D".to_string(),
        _ => "F".to_string(),
    }
}

/// Generate actionable recommendations based on violations and metrics.
fn generate_recommendations(
    _dep_graph: &DependencyGraph,
    violations: &[Violation],
    metrics: &[ModuleMetrics],
) -> Vec<Recommendation> {
    let mut recs = Vec::new();
    let mut priority = 1u32;

    // Cycles get highest priority.
    for v in violations.iter().filter(|v| v.rule == "circular_dependency") {
        recs.push(Recommendation {
            priority,
            category: "decouple".to_string(),
            action: format!(
                "Remove circular dependency between {}. Extract shared types into a new module.",
                v.modules.join(" and "),
            ),
            modules: v.modules.clone(),
        });
        priority += 1;
    }

    // God objects.
    for v in violations.iter().filter(|v| v.rule == "god_object") {
        recs.push(Recommendation {
            priority,
            category: "split".to_string(),
            action: format!(
                "Split {} into smaller, focused modules with single responsibilities.",
                v.modules.first().unwrap_or(&"unknown".to_string()),
            ),
            modules: v.modules.clone(),
        });
        priority += 1;
    }

    // Layer violations.
    for v in violations.iter().filter(|v| v.rule == "layer_violation") {
        recs.push(Recommendation {
            priority,
            category: "restructure".to_string(),
            action: v.suggestion.clone(),
            modules: v.modules.clone(),
        });
        priority += 1;
    }

    // High instability modules with high fan-out.
    for m in metrics
        .iter()
        .filter(|m| m.instability > 0.8 && m.efferent_coupling > 5)
    {
        recs.push(Recommendation {
            priority,
            category: "restructure".to_string(),
            action: format!(
                "Stabilize {} — depends on {} other modules. Consider abstracting behind traits.",
                m.module_name, m.efferent_coupling,
            ),
            modules: vec![m.module_name.clone()],
        });
        priority += 1;
    }

    recs
}
