//! Anti-pattern detection engine.
//!
//! Detects:
//! - Circular dependencies (cycles in the module graph)
//! - God Objects (files with excessive symbols or high fan-in + size)
//! - Layer violations (configurable via DNA)

use petgraph::algo::tarjan_scc;
use serde::{Deserialize, Serialize};

use crate::analyzer::DependencyGraph;

/// Severity of an architectural violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationSeverity {
    Warning,
    Error,
    Critical,
}

/// A detected architectural anti-pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Violation {
    /// Unique identifier for the violation type.
    pub rule: String,
    /// Human-readable description.
    pub description: String,
    /// Severity level.
    pub severity: ViolationSeverity,
    /// Modules involved.
    pub modules: Vec<String>,
    /// Suggested remediation.
    pub suggestion: String,
}

/// Thresholds for god-object detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GodObjectThresholds {
    pub(crate) max_public_symbols: usize,
    pub(crate) max_lines: usize,
    pub(crate) min_fan_in: usize,
}

impl Default for GodObjectThresholds {
    fn default() -> Self {
        Self {
            max_public_symbols: 50,
            max_lines: 1000,
            min_fan_in: 5,
        }
    }
}

/// Layer definition for violation detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LayerDefinition {
    pub(crate) name: String,
    pub(crate) rank: u32,
    pub(crate) modules: Vec<String>,
}

/// Thresholds for topological density anomaly detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DensityThresholds {
    /// Density above this triggers a warning (default: 0.5).
    pub(crate) high: f64,
    /// Density below this (with enough modules) triggers a warning (default: 0.02).
    pub(crate) low: f64,
    /// Minimum module count before low-density check kicks in (default: 10).
    pub(crate) low_min_modules: usize,
}

impl Default for DensityThresholds {
    fn default() -> Self {
        Self {
            high: 0.5,
            low: 0.02,
            low_min_modules: 10,
        }
    }
}

/// Configuration for the architectural linter.
#[derive(Debug, Clone, Default)]
pub struct LinterConfig {
    pub(crate) layers: Vec<LayerDefinition>,
    pub(crate) god_object: GodObjectThresholds,
    pub(crate) density: DensityThresholds,
}

impl LinterConfig {
    /// Build from DNA ArchitectConfig.
    pub fn from_dna(dna: &synapseed_core::liquid::ArchitectConfig) -> Self {
        let layers = dna
            .layers
            .iter()
            .map(|l| LayerDefinition {
                name: l.name.clone(),
                rank: l.rank,
                modules: l.modules.clone(),
            })
            .collect();

        let god_object = GodObjectThresholds {
            max_public_symbols: dna.god_object_max_symbols.unwrap_or(50),
            max_lines: dna.god_object_max_lines.unwrap_or(1000),
            min_fan_in: dna.god_object_min_fan_in.unwrap_or(5),
        };

        let density = DensityThresholds {
            high: dna.density_high_threshold.unwrap_or(0.5),
            low: dna.density_low_threshold.unwrap_or(0.02),
            low_min_modules: dna.density_low_min_modules.unwrap_or(10),
        };

        Self {
            layers,
            god_object,
            density,
        }
    }
}

/// Run all anti-pattern detectors and return violations.
pub fn lint(dep_graph: &DependencyGraph, config: &LinterConfig) -> Vec<Violation> {
    let mut violations = Vec::new();
    violations.extend(detect_cycles(dep_graph));
    violations.extend(detect_god_objects(dep_graph, &config.god_object));
    violations.extend(detect_layer_violations(dep_graph, &config.layers));
    violations.extend(detect_density_anomaly(dep_graph, &config.density));
    violations
}

/// Detect circular dependencies using Tarjan's SCC algorithm.
pub(crate) fn detect_cycles(dep_graph: &DependencyGraph) -> Vec<Violation> {
    let sccs = tarjan_scc(dep_graph.raw_graph());
    let mut violations = Vec::new();

    for scc in sccs {
        if scc.len() > 1 {
            let module_names: Vec<String> = scc
                .iter()
                .map(|idx| dep_graph.node(*idx).name.clone())
                .collect();

            let cycle_str = module_names.join(" → ");
            violations.push(Violation {
                rule: "circular_dependency".to_string(),
                description: format!(
                    "🔄 Circular dependency: {} → {} — these modules import each other, \
                     creating a loop that makes them impossible to change independently.",
                    cycle_str,
                    module_names.first().unwrap_or(&String::new())
                ),
                severity: ViolationSeverity::Error,
                modules: module_names.clone(),
                suggestion: format!(
                    "Action: Break the loop — extract the shared types that {} and {} both need \
                     into a new module, or use a trait (dependency inversion) so one side \
                     depends on an abstraction instead of the concrete implementation.",
                    module_names.first().unwrap_or(&String::new()),
                    module_names.last().unwrap_or(&String::new()),
                ),
            });
        }
    }

    violations
}

/// Detect god objects: files with >max_public_symbols OR
/// (>max_lines AND fan_in > min_fan_in).
pub(crate) fn detect_god_objects(
    dep_graph: &DependencyGraph,
    thresholds: &GodObjectThresholds,
) -> Vec<Violation> {
    let mut violations = Vec::new();

    for m in dep_graph.all_metrics() {
        if let Some(idx) = dep_graph.node_index(&m.module_name) {
            let node = dep_graph.node(idx);

            let too_many_symbols = node.public_symbol_count > thresholds.max_public_symbols;
            let too_large_and_popular =
                node.approx_lines > thresholds.max_lines && m.fan_in >= thresholds.min_fan_in;

            if too_many_symbols || too_large_and_popular {
                let reason = if too_many_symbols {
                    format!(
                        "🚨 {} is a monolith — it exposes {} public symbols (limit: {}). \
                         When a single file does everything, every change risks breaking something else.",
                        m.module_name, node.public_symbol_count, thresholds.max_public_symbols
                    )
                } else {
                    format!(
                        "🚨 {} is a God Object — ~{} lines with {} modules depending on it \
                         (limits: {} lines, {} dependents). It's doing too much.",
                        m.module_name,
                        node.approx_lines,
                        m.fan_in,
                        thresholds.max_lines,
                        thresholds.min_fan_in
                    )
                };

                violations.push(Violation {
                    rule: "god_object".to_string(),
                    description: format!("God Object detected: {reason}"),
                    severity: ViolationSeverity::Warning,
                    modules: vec![m.module_name.clone()],
                    suggestion: format!(
                        "Action: Split {} into smaller, focused modules — each doing ONE thing well. \
                         Look for logical clusters of functions and extract them.",
                        m.module_name
                    ),
                });
            }
        }
    }

    violations
}

/// Detect layer violations: a lower-rank layer module importing from a
/// higher-rank layer module.
pub(crate) fn detect_layer_violations(
    dep_graph: &DependencyGraph,
    layers: &[LayerDefinition],
) -> Vec<Violation> {
    if layers.is_empty() {
        return Vec::new();
    }

    let mut violations = Vec::new();

    // Build module -> layer rank lookup.
    let mut module_layer: std::collections::HashMap<String, (String, u32)> =
        std::collections::HashMap::new();
    for layer in layers {
        for pattern in &layer.modules {
            // Simple glob: if pattern ends with *, prefix match; otherwise exact.
            // We record the pattern -> (layer_name, rank).
            module_layer.insert(pattern.clone(), (layer.name.clone(), layer.rank));
        }
    }

    // Resolve module name to its layer rank.
    let resolve_layer = |module_name: &str| -> Option<(String, u32)> {
        // Exact match first.
        if let Some(lr) = module_layer.get(module_name) {
            return Some(lr.clone());
        }
        // Prefix match (patterns ending with *).
        for (pattern, lr) in &module_layer {
            if let Some(prefix) = pattern.strip_suffix('*') {
                if module_name.starts_with(prefix) {
                    return Some(lr.clone());
                }
            }
        }
        None
    };

    for (source, target, _edge) in dep_graph.all_edges() {
        if let (Some((src_layer, src_rank)), Some((tgt_layer, tgt_rank))) =
            (resolve_layer(&source), resolve_layer(&target))
        {
            // Lower rank importing from higher rank = violation.
            if src_rank < tgt_rank {
                violations.push(Violation {
                    rule: "layer_violation".to_string(),
                    description: format!(
                        "⛔ Layer violation: {source} ({src_layer}) is importing from \
                         {target} ({tgt_layer}) — a lower layer is reaching into a higher one. \
                         This breaks the layered architecture."
                    ),
                    severity: ViolationSeverity::Error,
                    modules: vec![source.clone(), target.clone()],
                    suggestion: format!(
                        "Action: Move the needed logic down to {src_layer}, \
                         or define a trait in {src_layer} that {tgt_layer} implements — \
                         so the dependency arrow points downward, not upward."
                    ),
                });
            }
        }
    }

    violations
}

/// Detect topological density anomalies.
///
/// D = E / (V × (V − 1)) for directed graphs.
/// - D > high → Warning: over-connected graph, hard to evolve.
/// - D < low (with V ≥ min_modules) → Warning: fragmented, possibly missing deps.
pub(crate) fn detect_density_anomaly(
    dep_graph: &DependencyGraph,
    thresholds: &DensityThresholds,
) -> Vec<Violation> {
    let density = dep_graph.topological_density();
    let v = dep_graph.module_count();
    let mut violations = Vec::new();

    if density > thresholds.high {
        violations.push(Violation {
            rule: "high_density".to_string(),
            description: format!(
                "⚠️ Your module graph is tangled — density {density:.4} exceeds {:.2} \
                 ({} modules, {} edges). Almost every module depends on every other. \
                 Changes will ripple everywhere.",
                thresholds.high,
                v,
                dep_graph.edge_count(),
            ),
            severity: ViolationSeverity::Warning,
            modules: vec![],
            suggestion: "Action: Introduce facade modules as chokepoints — group related modules \
                         behind a single public API, reducing cross-dependencies. \
                         Think \"firewall between neighborhoods\"."
                .to_string(),
        });
    } else if v >= thresholds.low_min_modules && density < thresholds.low {
        violations.push(Violation {
            rule: "low_density".to_string(),
            description: format!(
                "⚠️ Your modules are isolated islands — density {density:.4} is below {:.2} \
                 ({} modules, {} edges). Very few connections means modules may be \
                 duplicating logic or have lost shared abstractions.",
                thresholds.low,
                v,
                dep_graph.edge_count(),
            ),
            severity: ViolationSeverity::Warning,
            modules: vec![],
            suggestion: "Action: Look for repeated patterns across modules and extract shared \
                         abstractions. If modules truly have nothing in common, \
                         consider whether they belong in the same project."
                .to_string(),
        });
    }

    violations
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapseed_cortex::graph::CodeGraph;

    #[test]
    fn test_god_object_low_threshold() {
        let project_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();

        let graph = CodeGraph::new();
        graph.index_directory(project_root).unwrap();

        let mut dep_graph = DependencyGraph::build(&graph);
        dep_graph.compute_metrics();

        let strict_thresholds = GodObjectThresholds {
            max_public_symbols: 3,
            max_lines: 50,
            min_fan_in: 0,
        };

        let violations = detect_god_objects(&dep_graph, &strict_thresholds);
        assert!(
            !violations.is_empty(),
            "With max_public_symbols=3, expected some god objects in synapseed"
        );
        assert!(violations.iter().all(|v| v.rule == "god_object"));
        assert!(violations
            .iter()
            .all(|v| v.severity == ViolationSeverity::Warning));
    }
}
