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
pub struct GodObjectThresholds {
    pub max_public_symbols: usize,
    pub max_lines: usize,
    pub min_fan_in: usize,
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
pub struct LayerDefinition {
    /// Layer name (e.g., "core", "domain", "api", "ui").
    pub name: String,
    /// Layer rank (0 = bottom). Lower must not import from higher.
    pub rank: u32,
    /// Module name patterns belonging to this layer.
    pub modules: Vec<String>,
}

/// Configuration for the architectural linter.
#[derive(Debug, Clone, Default)]
pub struct LinterConfig {
    pub layers: Vec<LayerDefinition>,
    pub god_object: GodObjectThresholds,
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

        Self { layers, god_object }
    }
}

/// Run all anti-pattern detectors and return violations.
pub fn lint(dep_graph: &DependencyGraph, config: &LinterConfig) -> Vec<Violation> {
    let mut violations = Vec::new();
    violations.extend(detect_cycles(dep_graph));
    violations.extend(detect_god_objects(dep_graph, &config.god_object));
    violations.extend(detect_layer_violations(dep_graph, &config.layers));
    violations
}

/// Detect circular dependencies using Tarjan's SCC algorithm.
pub fn detect_cycles(dep_graph: &DependencyGraph) -> Vec<Violation> {
    let sccs = tarjan_scc(dep_graph.raw_graph());
    let mut violations = Vec::new();

    for scc in sccs {
        if scc.len() > 1 {
            let module_names: Vec<String> = scc
                .iter()
                .map(|idx| dep_graph.node(*idx).name.clone())
                .collect();

            let cycle_str = module_names.join(" -> ");
            violations.push(Violation {
                rule: "circular_dependency".to_string(),
                description: format!(
                    "Circular dependency detected: {} -> {}",
                    cycle_str,
                    module_names.first().unwrap_or(&String::new())
                ),
                severity: ViolationSeverity::Error,
                modules: module_names.clone(),
                suggestion: format!(
                    "Break the cycle by extracting shared types into a separate module, \
                     or use dependency inversion (traits) between {} and {}.",
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
pub fn detect_god_objects(
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
                        "{} has {} public symbols (threshold: {})",
                        m.module_name, node.public_symbol_count, thresholds.max_public_symbols
                    )
                } else {
                    format!(
                        "{} has ~{} lines and fan-in {} (thresholds: {} lines, {} fan-in)",
                        m.module_name, node.approx_lines, m.fan_in,
                        thresholds.max_lines, thresholds.min_fan_in
                    )
                };

                violations.push(Violation {
                    rule: "god_object".to_string(),
                    description: format!("God Object detected: {reason}"),
                    severity: ViolationSeverity::Warning,
                    modules: vec![m.module_name.clone()],
                    suggestion: format!(
                        "Split {} into smaller, focused modules with single responsibilities.",
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
pub fn detect_layer_violations(
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
                        "Layer violation: {source} (layer: {src_layer}, rank {src_rank}) \
                         imports from {target} (layer: {tgt_layer}, rank {tgt_rank})"
                    ),
                    severity: ViolationSeverity::Error,
                    modules: vec![source.clone(), target.clone()],
                    suggestion: format!(
                        "Move the dependency from {source} to {target} behind a trait, \
                         or restructure so {src_layer} does not depend on {tgt_layer}."
                    ),
                });
            }
        }
    }

    violations
}
