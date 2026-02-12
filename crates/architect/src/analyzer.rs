//! Module dependency graph builder and structural metrics calculator.

use std::collections::HashMap;
use std::path::Path;

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use serde::{Deserialize, Serialize};

use synapseed_core::symbol::SymbolKind;
use synapseed_cortex::graph::CodeGraph;

/// A module node in the dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModuleNode {
    pub(crate) name: String,
    pub(crate) file_path: String,
    pub(crate) language: String,
    pub(crate) public_symbol_count: usize,
    pub(crate) approx_lines: usize,
    pub(crate) function_count: usize,
}

/// A directed edge representing a dependency: source imports from target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DependencyEdge {
    pub(crate) import_signature: String,
    pub(crate) weight: usize,
}

/// Metrics for a single module in the dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleMetrics {
    pub module_name: String,
    pub file_path: String,
    /// Number of outgoing edges (modules this module depends on).
    pub efferent_coupling: usize,
    /// Number of incoming edges (modules that depend on this module).
    pub afferent_coupling: usize,
    /// Instability = Ce / (Ca + Ce). Range [0.0, 1.0].
    /// 0.0 = maximally stable, 1.0 = maximally unstable.
    pub instability: f64,
    /// Approximate complexity = function_count.
    pub approx_complexity: usize,
    /// Fan-in: total weight of incoming dependency edges.
    pub fan_in: usize,
    /// Fan-out: total weight of outgoing dependency edges.
    pub fan_out: usize,
}

/// The module dependency graph with computed metrics.
pub struct DependencyGraph {
    graph: DiGraph<ModuleNode, DependencyEdge>,
    node_map: HashMap<String, NodeIndex>,
    metrics: Vec<ModuleMetrics>,
}

impl DependencyGraph {
    /// Build the dependency graph from a CodeGraph.
    pub fn build(code_graph: &CodeGraph) -> Self {
        let mut graph = DiGraph::new();
        let mut node_map: HashMap<String, NodeIndex> = HashMap::new();

        // Phase 1: Create a node for each file.
        // Module key = crate-qualified path to avoid collisions
        // (e.g., "cortex::graph", "mcp::tools", "search::plugin").
        for file in code_graph.all_files() {
            let module_name = derive_module_name(&file.path);

            let public_symbol_count = file
                .symbols
                .iter()
                .filter(|s| s.kind != SymbolKind::Import)
                .count();

            let approx_lines = file
                .symbols
                .iter()
                .map(|s| s.line_end)
                .max()
                .unwrap_or(0);

            let function_count = file
                .symbols
                .iter()
                .filter(|s| matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
                .count();

            let node = ModuleNode {
                name: module_name.clone(),
                file_path: file.path.clone(),
                language: file.language.clone(),
                public_symbol_count,
                approx_lines,
                function_count,
            };

            let idx = graph.add_node(node);
            // If multiple files map to same module name, keep the first.
            node_map.entry(module_name).or_insert(idx);
        }

        // Phase 2: Create edges from Import symbols.
        for file in code_graph.all_files() {
            let source_name = derive_module_name(&file.path);

            let source_idx = match node_map.get(&source_name) {
                Some(idx) => *idx,
                None => continue,
            };

            // Track edge weights (multiple imports to same target).
            let mut edge_weights: HashMap<String, (String, usize)> = HashMap::new();

            for sym in &file.symbols {
                if sym.kind != SymbolKind::Import {
                    continue;
                }

                let sig = sym.signature.as_deref().unwrap_or("");
                if let Some(target) = parse_import_target(sig, &file.language) {
                    // Skip self-imports.
                    if target == source_name {
                        continue;
                    }
                    let entry = edge_weights
                        .entry(target)
                        .or_insert_with(|| (sig.to_string(), 0));
                    entry.1 += 1;
                }
            }

            for (target_stem, (sig, weight)) in edge_weights {
                // Find the target node: look for exact match first, then
                // suffix match (e.g., target "graph" matches "cortex::graph").
                let target_idx = node_map
                    .get(&target_stem)
                    .copied()
                    .or_else(|| {
                        // For crate::module imports, try matching same-crate modules first.
                        let source_crate = source_name.split("::").next().unwrap_or("");
                        let same_crate_key = format!("{source_crate}::{target_stem}");
                        node_map.get(&same_crate_key).copied()
                    })
                    .or_else(|| {
                        // Fallback: any module ending with ::target_stem.
                        node_map
                            .iter()
                            .find(|(k, _)| {
                                k.ends_with(&format!("::{target_stem}"))
                                    || k.as_str() == target_stem
                            })
                            .map(|(_, v)| *v)
                    });

                if let Some(target_idx) = target_idx {
                    if target_idx != source_idx {
                        graph.add_edge(
                            source_idx,
                            target_idx,
                            DependencyEdge {
                                import_signature: sig,
                                weight,
                            },
                        );
                    }
                }
            }
        }

        Self {
            graph,
            node_map,
            metrics: Vec::new(),
        }
    }

    /// Compute structural metrics for all modules.
    pub fn compute_metrics(&mut self) {
        let mut metrics = Vec::new();

        for (name, &idx) in &self.node_map {
            let node = &self.graph[idx];

            let efferent: usize = self
                .graph
                .edges_directed(idx, Direction::Outgoing)
                .count();
            let afferent: usize = self
                .graph
                .edges_directed(idx, Direction::Incoming)
                .count();

            let instability = if efferent + afferent == 0 {
                0.0
            } else {
                efferent as f64 / (afferent + efferent) as f64
            };

            let fan_out: usize = self
                .graph
                .edges_directed(idx, Direction::Outgoing)
                .map(|e| e.weight().weight)
                .sum();
            let fan_in: usize = self
                .graph
                .edges_directed(idx, Direction::Incoming)
                .map(|e| e.weight().weight)
                .sum();

            metrics.push(ModuleMetrics {
                module_name: name.clone(),
                file_path: node.file_path.clone(),
                efferent_coupling: efferent,
                afferent_coupling: afferent,
                instability,
                approx_complexity: node.function_count,
                fan_in,
                fan_out,
            });
        }

        // Sort by module_name for deterministic output across runs.
        metrics.sort_by(|a, b| a.module_name.cmp(&b.module_name));
        self.metrics = metrics;
    }

    /// Get metrics for all modules.
    pub fn all_metrics(&self) -> &[ModuleMetrics] {
        &self.metrics
    }

    /// Get metrics for a specific module by name.
    pub fn module_metrics(&self, name: &str) -> Option<&ModuleMetrics> {
        self.metrics.iter().find(|m| m.module_name == name)
    }

    /// Get all edges (source_name, target_name, edge).
    pub(crate) fn all_edges(&self) -> Vec<(String, String, &DependencyEdge)> {
        self.graph
            .edge_indices()
            .filter_map(|e| {
                let (src, tgt) = self.graph.edge_endpoints(e)?;
                let edge = &self.graph[e];
                Some((
                    self.graph[src].name.clone(),
                    self.graph[tgt].name.clone(),
                    edge,
                ))
            })
            .collect()
    }

    /// Get the total number of modules.
    pub fn module_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Get the total number of dependency edges.
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Compute topological density: D = E / (V × (V − 1)) for directed graphs.
    ///
    /// Range [0.0, 1.0]. Returns 0.0 if the graph has fewer than 2 nodes.
    pub fn topological_density(&self) -> f64 {
        let v = self.graph.node_count();
        if v <= 1 {
            return 0.0;
        }
        let e = self.graph.edge_count();
        e as f64 / (v as f64 * (v as f64 - 1.0))
    }

    /// Get the raw petgraph (for cycle detection in linter).
    pub(crate) fn raw_graph(&self) -> &DiGraph<ModuleNode, DependencyEdge> {
        &self.graph
    }

    /// Get the NodeIndex for a module name.
    pub(crate) fn node_index(&self, name: &str) -> Option<NodeIndex> {
        self.node_map.get(name).copied()
    }

    /// Get the ModuleNode at a given NodeIndex.
    pub(crate) fn node(&self, idx: NodeIndex) -> &ModuleNode {
        &self.graph[idx]
    }
}

/// Derive a crate-qualified module name from a file path.
///
/// Examples:
/// - `crates/cortex/src/graph.rs` → `cortex::graph`
/// - `crates/mcp/src/tools.rs` → `mcp::tools`
/// - `src/lib.rs` → `lib`
/// - `crates/core/src/lib.rs` → `core::lib`
/// - `auth/views.py` → `auth::views`
fn derive_module_name(file_path: &str) -> String {
    let path = Path::new(file_path);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    // Walk up from the file to find a crate context.
    // Pattern: look for a "src" directory — its parent is the crate.
    let components: Vec<&str> = path
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();

    // Find the last "src" directory in the path.
    if let Some(src_pos) = components.iter().rposition(|c| *c == "src") {
        if src_pos > 0 {
            let crate_name = components[src_pos - 1];
            return format!("{crate_name}::{stem}");
        }
    }

    // Fallback: use parent dir + stem.
    if let Some(parent) = path.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str()) {
        if parent != "src" && parent != "." {
            return format!("{parent}::{stem}");
        }
    }

    stem.to_string()
}

/// Parse an import signature to extract the target module name.
///
/// Returns None for external crate imports (std::, third-party).
pub(crate) fn parse_import_target(signature: &str, language: &str) -> Option<String> {
    let sig = signature.trim();
    match language {
        "rust" => {
            // "use crate::module_name::..." -> "module_name"
            if let Some(rest) = sig.strip_prefix("use crate::") {
                let module = rest.split("::").next()?;
                let module = module.trim_end_matches(';').trim_end_matches('{').trim();
                if module.is_empty() {
                    return None;
                }
                Some(module.to_string())
            }
            // "use super::module_name" -> "module_name"
            else if let Some(rest) = sig.strip_prefix("use super::") {
                let module = rest.split("::").next()?;
                let module = module.trim_end_matches(';').trim_end_matches('{').trim();
                if module.is_empty() {
                    return None;
                }
                Some(module.to_string())
            } else {
                None // external crate import, skip
            }
        }
        "python" => {
            if let Some(rest) = sig.strip_prefix("from ") {
                let module = rest.split_whitespace().next()?;
                let module = module.split('.').next()?;
                if module.is_empty() {
                    return None;
                }
                Some(module.to_string())
            } else if let Some(rest) = sig.strip_prefix("import ") {
                let module = rest.split_whitespace().next()?;
                let module = module.split('.').next()?;
                if module.is_empty() {
                    return None;
                }
                Some(module.to_string())
            } else {
                None
            }
        }
        "javascript" => {
            if let Some(from_idx) = sig.find("from ") {
                let after_from = &sig[from_idx + 5..];
                let path = after_from
                    .trim()
                    .trim_matches(|c| c == '\'' || c == '"' || c == ';' || c == ' ');
                // Only resolve relative imports (./foo, ../foo)
                if !path.starts_with('.') {
                    return None;
                }
                let stem = Path::new(path).file_stem()?.to_str()?;
                if stem.is_empty() {
                    return None;
                }
                Some(stem.to_string())
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_import_rust_crate() {
        assert_eq!(
            parse_import_target("use crate::auth::User;", "rust"),
            Some("auth".to_string())
        );
        assert_eq!(
            parse_import_target("use crate::graph::CodeGraph;", "rust"),
            Some("graph".to_string())
        );
        assert_eq!(
            parse_import_target("use crate::protocol::{ContentBlock, ToolCallResult};", "rust"),
            Some("protocol".to_string())
        );
    }

    #[test]
    fn test_parse_import_rust_super() {
        assert_eq!(
            parse_import_target("use super::utils;", "rust"),
            Some("utils".to_string())
        );
        assert_eq!(
            parse_import_target("use super::config::Settings;", "rust"),
            Some("config".to_string())
        );
    }

    #[test]
    fn test_parse_import_rust_external_skipped() {
        assert_eq!(parse_import_target("use std::collections::HashMap;", "rust"), None);
        assert_eq!(parse_import_target("use serde::Serialize;", "rust"), None);
        assert_eq!(parse_import_target("use tokio::sync::Mutex;", "rust"), None);
    }

    #[test]
    fn test_parse_import_python() {
        assert_eq!(
            parse_import_target("from auth import User", "python"),
            Some("auth".to_string())
        );
        assert_eq!(
            parse_import_target("import utils", "python"),
            Some("utils".to_string())
        );
        assert_eq!(
            parse_import_target("from os.path import join", "python"),
            Some("os".to_string())
        );
    }

    #[test]
    fn test_parse_import_javascript() {
        assert_eq!(
            parse_import_target("import { foo } from './auth';", "javascript"),
            Some("auth".to_string())
        );
        assert_eq!(
            parse_import_target("import bar from '../utils';", "javascript"),
            Some("utils".to_string())
        );
        // External import — skip
        assert_eq!(
            parse_import_target("import React from 'react';", "javascript"),
            None
        );
    }
}
