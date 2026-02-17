//! PageRank computation on the module dependency graph (v4.8.0).
//!
//! Uses classic power iteration with damping factor d=0.85.
//! Scores are normalized to [0.0, 1.0] where 1.0 = most depended-upon module.
//!
//! Higher PageRank = module is imported by many other modules → its symbols
//! are foundational and should rank higher in search results.

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;

use super::analyzer::{DependencyEdge, ModuleNode};

/// Compute PageRank scores for all nodes in a directed graph.
///
/// Classic power iteration: `PR(v) = (1-d)/N + d × Σ(PR(u)/out(u))` for all u→v.
/// Converges when max per-node change < epsilon (1e-6), or after `max_iterations`.
///
/// Returns (NodeIndex, score) pairs with scores normalized to [0.0, 1.0].
pub(crate) fn compute(
    graph: &DiGraph<ModuleNode, DependencyEdge>,
    damping: f64,
    max_iterations: usize,
) -> Vec<(NodeIndex, f64)> {
    let n = graph.node_count();
    if n == 0 {
        return Vec::new();
    }

    let n_f = n as f64;
    let base = (1.0 - damping) / n_f;
    let indices: Vec<NodeIndex> = graph.node_indices().collect();

    // Initialize uniform scores
    let mut scores: Vec<f64> = vec![1.0 / n_f; n];

    // Precompute outgoing edge counts for efficiency
    let out_degree: Vec<usize> = indices
        .iter()
        .map(|&idx| graph.edges_directed(idx, Direction::Outgoing).count())
        .collect();

    for _ in 0..max_iterations {
        let mut new_scores = vec![base; n];
        let mut max_delta = 0.0f64;

        for (i, &node) in indices.iter().enumerate() {
            let mut sum = 0.0;
            for incoming in graph.neighbors_directed(node, Direction::Incoming) {
                let j = incoming.index();
                if out_degree[j] > 0 {
                    sum += scores[j] / out_degree[j] as f64;
                }
            }
            new_scores[i] = base + damping * sum;
            let delta = (new_scores[i] - scores[i]).abs();
            if delta > max_delta {
                max_delta = delta;
            }
        }

        scores = new_scores;
        if max_delta < 1e-6 {
            break;
        }
    }

    // Normalize to [0, 1]: divide by max score
    let max_score = scores.iter().cloned().fold(0.0f64, f64::max);
    if max_score > 0.0 {
        for score in &mut scores {
            *score /= max_score;
        }
    }

    indices.into_iter().zip(scores).collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn make_node(name: &str) -> ModuleNode {
        ModuleNode {
            name: name.to_string(),
            file_path: format!("src/{name}.rs"),
            language: "rust".to_string(),
            public_symbol_count: 1,
            approx_lines: 100,
            function_count: 5,
        }
    }

    fn make_edge() -> DependencyEdge {
        DependencyEdge {
            import_signature: "use crate::test".to_string(),
            weight: 1,
        }
    }

    #[test]
    fn test_empty_graph() {
        let graph: DiGraph<ModuleNode, DependencyEdge> = DiGraph::new();
        let scores = compute(&graph, 0.85, 100);
        assert!(scores.is_empty());
    }

    #[test]
    fn test_single_node() {
        let mut graph = DiGraph::new();
        graph.add_node(make_node("core"));
        let scores = compute(&graph, 0.85, 100);
        assert_eq!(scores.len(), 1);
        assert!((scores[0].1 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_star_topology_hub_wins() {
        // A→C, B→C, D→C — C is the hub (most imported)
        let mut graph = DiGraph::new();
        let a = graph.add_node(make_node("a"));
        let b = graph.add_node(make_node("b"));
        let c = graph.add_node(make_node("c"));
        let d = graph.add_node(make_node("d"));
        graph.add_edge(a, c, make_edge());
        graph.add_edge(b, c, make_edge());
        graph.add_edge(d, c, make_edge());

        let scores = compute(&graph, 0.85, 100);
        let score_map: HashMap<NodeIndex, f64> = scores.into_iter().collect();

        // C should have the highest score (normalized to 1.0)
        assert!(
            (score_map[&c] - 1.0).abs() < 1e-6,
            "Hub C should have score 1.0, got {}",
            score_map[&c]
        );
        // Leaf nodes should have lower scores
        assert!(score_map[&a] < score_map[&c]);
        assert!(score_map[&b] < score_map[&c]);
        assert!(score_map[&d] < score_map[&c]);
    }

    #[test]
    fn test_linear_chain() {
        // A→B→C — C is most depended upon (transitively)
        let mut graph = DiGraph::new();
        let a = graph.add_node(make_node("a"));
        let b = graph.add_node(make_node("b"));
        let c = graph.add_node(make_node("c"));
        graph.add_edge(a, b, make_edge());
        graph.add_edge(b, c, make_edge());

        let scores = compute(&graph, 0.85, 100);
        let score_map: HashMap<NodeIndex, f64> = scores.into_iter().collect();

        // C > B > A
        assert!(
            score_map[&c] > score_map[&b],
            "C ({}) should rank higher than B ({})",
            score_map[&c],
            score_map[&b]
        );
        assert!(
            score_map[&b] > score_map[&a],
            "B ({}) should rank higher than A ({})",
            score_map[&b],
            score_map[&a]
        );
    }

    #[test]
    fn test_cycle_converges_to_equal() {
        // A→B→A — mutual dependency, should converge to equal scores
        let mut graph = DiGraph::new();
        let a = graph.add_node(make_node("a"));
        let b = graph.add_node(make_node("b"));
        graph.add_edge(a, b, make_edge());
        graph.add_edge(b, a, make_edge());

        let scores = compute(&graph, 0.85, 100);
        let score_map: HashMap<NodeIndex, f64> = scores.into_iter().collect();

        assert!(
            (score_map[&a] - score_map[&b]).abs() < 1e-3,
            "Mutual dependency should yield equal scores: A={}, B={}",
            score_map[&a],
            score_map[&b]
        );
    }

    #[test]
    fn test_scores_normalized_to_unit() {
        let mut graph = DiGraph::new();
        let a = graph.add_node(make_node("a"));
        let b = graph.add_node(make_node("b"));
        let c = graph.add_node(make_node("c"));
        graph.add_edge(a, b, make_edge());
        graph.add_edge(a, c, make_edge());
        graph.add_edge(b, c, make_edge());

        let scores = compute(&graph, 0.85, 100);

        for (_, score) in &scores {
            assert!(
                *score >= 0.0 && *score <= 1.0,
                "Score {} out of [0, 1] range",
                score
            );
        }

        let max = scores.iter().map(|(_, s)| *s).fold(0.0f64, f64::max);
        assert!(
            (max - 1.0).abs() < 1e-6,
            "Max score should be 1.0, got {}",
            max
        );
    }
}
