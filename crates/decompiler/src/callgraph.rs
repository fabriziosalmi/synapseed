//! Call graph construction from symbol cross-references.

use serde::Serialize;

use crate::binary::ExportedSymbol;

/// A directed call graph between functions.
#[derive(Debug, Serialize)]
pub struct CallGraph {
    /// Nodes (function symbols).
    pub nodes: Vec<CallNode>,
    /// Directed edges (caller → callee).
    pub edges: Vec<CallEdge>,
    /// Number of connected components.
    pub components: usize,
    /// Leaf functions (no outgoing calls).
    pub leaf_count: usize,
    /// Root functions (no incoming calls).
    pub root_count: usize,
}

/// A function node in the call graph.
#[derive(Debug, Clone, Serialize)]
pub struct CallNode {
    /// Symbol index.
    pub id: usize,
    /// Function name.
    pub name: String,
    /// Demangled name if available.
    pub demangled: Option<String>,
    /// Address.
    pub address: u64,
    /// Number of outgoing calls.
    pub out_degree: usize,
    /// Number of incoming calls.
    pub in_degree: usize,
}

/// A directed edge: function A calls function B.
#[derive(Debug, Clone, Serialize)]
pub struct CallEdge {
    /// Caller node index.
    pub from: usize,
    /// Callee node index.
    pub to: usize,
}

/// Build a call graph from symbols.
///
/// Note: Without disassembly, we use heuristic proximity analysis.
/// Functions that are imported are marked as callees of nearby local functions.
/// This provides a reasonable approximation of the actual call graph.
pub fn build_call_graph(symbols: &[ExportedSymbol]) -> CallGraph {
    use crate::binary::SymbolKind;

    // Only include function symbols
    let functions: Vec<(usize, &ExportedSymbol)> = symbols
        .iter()
        .enumerate()
        .filter(|(_, s)| s.kind == SymbolKind::Function)
        .collect();

    if functions.is_empty() {
        return CallGraph {
            nodes: Vec::new(),
            edges: Vec::new(),
            components: 0,
            leaf_count: 0,
            root_count: 0,
        };
    }

    // Build nodes
    let nodes: Vec<CallNode> = functions
        .iter()
        .enumerate()
        .map(|(idx, (_, sym))| CallNode {
            id: idx,
            name: sym.name.clone(),
            demangled: sym.demangled.clone(),
            address: sym.address,
            out_degree: 0,
            in_degree: 0,
        })
        .collect();

    // Build edges using heuristic: imports are called by the nearest
    // preceding local function. This is a rough approximation.
    let mut edges = Vec::new();
    let local_idxs: Vec<usize> = functions
        .iter()
        .enumerate()
        .filter(|(_, (_, s))| !s.is_import)
        .map(|(idx, _)| idx)
        .collect();
    let import_idxs: Vec<usize> = functions
        .iter()
        .enumerate()
        .filter(|(_, (_, s))| s.is_import)
        .map(|(idx, _)| idx)
        .collect();

    // For each import, find the closest preceding local function
    for &imp_idx in &import_idxs {
        let imp_addr = nodes[imp_idx].address;
        let mut best: Option<(usize, u64)> = None;
        for &loc_idx in &local_idxs {
            let loc_addr = nodes[loc_idx].address;
            if loc_addr <= imp_addr {
                let dist = imp_addr - loc_addr;
                if best.is_none() || dist < best.map_or(u64::MAX, |b| b.1) {
                    best = Some((loc_idx, dist));
                }
            }
        }
        if let Some((caller, _)) = best {
            edges.push(CallEdge {
                from: caller,
                to: imp_idx,
            });
        }
    }

    // Calculate degrees
    let mut nodes = nodes;
    for edge in &edges {
        if edge.from < nodes.len() {
            nodes[edge.from].out_degree += 1;
        }
        if edge.to < nodes.len() {
            nodes[edge.to].in_degree += 1;
        }
    }

    let leaf_count = nodes
        .iter()
        .filter(|n| n.out_degree == 0 && !n.name.is_empty())
        .count();
    let root_count = nodes
        .iter()
        .filter(|n| n.in_degree == 0 && !n.name.is_empty())
        .count();

    // Simple component count via union-find
    let components = count_components(nodes.len(), &edges);

    CallGraph {
        nodes,
        edges,
        components,
        leaf_count,
        root_count,
    }
}

/// Count connected components using union-find.
fn count_components(n: usize, edges: &[CallEdge]) -> usize {
    if n == 0 {
        return 0;
    }
    let mut parent: Vec<usize> = (0..n).collect();

    fn find(parent: &mut [usize], x: usize) -> usize {
        if parent[x] != x {
            parent[x] = find(parent, parent[x]);
        }
        parent[x]
    }

    fn union(parent: &mut [usize], a: usize, b: usize) {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb {
            parent[ra] = rb;
        }
    }

    for edge in edges {
        if edge.from < n && edge.to < n {
            union(&mut parent, edge.from, edge.to);
        }
    }

    let mut roots = std::collections::HashSet::new();
    for i in 0..n {
        roots.insert(find(&mut parent, i));
    }
    roots.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binary::{ExportedSymbol, SymbolKind};

    fn make_sym(name: &str, addr: u64, is_import: bool) -> ExportedSymbol {
        ExportedSymbol {
            name: name.into(),
            demangled: None,
            kind: SymbolKind::Function,
            address: addr,
            size: 0,
            is_import,
        }
    }

    #[test]
    fn test_empty_call_graph() {
        let cg = build_call_graph(&[]);
        assert_eq!(cg.nodes.len(), 0);
        assert_eq!(cg.edges.len(), 0);
    }

    #[test]
    fn test_call_graph_with_imports() {
        let syms = vec![
            make_sym("main", 0x1000, false),
            make_sym("helper", 0x2000, false),
            make_sym("printf", 0x1500, true),
        ];
        let cg = build_call_graph(&syms);
        assert_eq!(cg.nodes.len(), 3);
        // printf should be linked to main (nearest preceding local)
        assert!(!cg.edges.is_empty());
    }
}
