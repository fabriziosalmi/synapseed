//! Coherence Gate (v4.2.0) — "The Cigarette Break"
//!
//! When the extraction pipeline produces targets scattered across many
//! unrelated modules, the LLM context becomes noisy and hallucination-prone.
//! The Coherence Gate detects this (via a Coherence Score) and reorders
//! targets by clustering them by module proximity, keeping only the most
//! relevant clusters.
//!
//! ## Coherence Score (CS)
//!
//! ```text
//! CS = 1 - (unique_modules - 1) / max(total_targets - 1, 1)
//! ```
//!
//! - CS = 1.0 → all targets in same module (perfectly coherent)
//! - CS = 0.0 → every target in a different module (maximally scattered)
//!
//! When CS < τ (default 0.4), the gate activates: clusters by module prefix,
//! ranks by cluster size, keeps top-K clusters (K depends on model tier).

use synapseed_core::momentum::ModelTier;
use tracing::debug;

use super::Target;

/// Extract module prefix from a relative file path.
///
/// `"crates/whisper/src/router/mod.rs"` → `"crates/whisper"`
/// `"src/main.rs"` → `"src"`
/// `""` → `""`
fn module_prefix(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() >= 2 {
        format!("{}/{}", parts[0], parts[1])
    } else {
        parts.first().unwrap_or(&"").to_string()
    }
}

/// Compute coherence score: 1.0 = all same module, 0.0 = maximally scattered.
fn coherence_score(targets: &[Target]) -> f64 {
    if targets.len() <= 1 {
        return 1.0;
    }
    let unique: std::collections::HashSet<String> = targets
        .iter()
        .map(|t| module_prefix(t.file_path.as_deref().unwrap_or("")))
        .collect();
    1.0 - (unique.len() as f64 - 1.0) / (targets.len() as f64 - 1.0).max(1.0)
}

const COHERENCE_THRESHOLD: f64 = 0.4;

/// Apply the Coherence Gate. Mutates `targets` in-place when CS < threshold.
///
/// - Atomic tier: keeps top 2 clusters
/// - Molecular/Galactic: keeps top 3 clusters
pub(super) fn coherence_gate(targets: &mut Vec<Target>, tier: ModelTier) {
    if targets.len() <= 2 {
        return; // Nothing to reorder with ≤2 targets
    }

    let cs = coherence_score(targets);
    if cs >= COHERENCE_THRESHOLD {
        debug!(cs = cs, "Coherence Gate: PASS (CS >= threshold)");
        return;
    }

    let max_clusters = match tier {
        ModelTier::Atomic => 2,
        _ => 3,
    };

    debug!(
        cs = cs,
        threshold = COHERENCE_THRESHOLD,
        max_clusters,
        "Coherence Gate: TRIGGERED — reordering targets"
    );

    // Cluster by module prefix, preserving insertion order within each cluster
    let mut clusters: Vec<(String, Vec<Target>)> = Vec::new();
    for target in targets.drain(..) {
        let prefix = module_prefix(target.file_path.as_deref().unwrap_or(""));
        if let Some(cluster) = clusters.iter_mut().find(|(p, _)| *p == prefix) {
            cluster.1.push(target);
        } else {
            clusters.push((prefix, vec![target]));
        }
    }

    // Sort clusters by size (largest first) — biggest cluster = most relevant
    clusters.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    // Keep top-K clusters, rebuild targets
    for (_prefix, cluster_targets) in clusters.into_iter().take(max_clusters) {
        targets.extend(cluster_targets);
    }

    let new_cs = coherence_score(targets);
    debug!(
        before_cs = cs,
        after_cs = new_cs,
        remaining = targets.len(),
        "Coherence Gate: reordering complete"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::TargetKind;

    fn make_target(name: &str, file: &str) -> Target {
        Target {
            kind: TargetKind::Symbol,
            name: name.to_string(),
            file_path: Some(file.to_string()),
            line_start: Some(1),
        }
    }

    #[test]
    fn test_module_prefix_extraction() {
        assert_eq!(module_prefix("crates/whisper/src/router/mod.rs"), "crates/whisper");
        assert_eq!(module_prefix("crates/core/src/lib.rs"), "crates/core");
        assert_eq!(module_prefix("src/main.rs"), "src/main.rs");
        assert_eq!(module_prefix("lib.rs"), "lib.rs");
        assert_eq!(module_prefix(""), "");
    }

    #[test]
    fn test_coherence_score_single_module() {
        let targets = vec![
            make_target("foo", "crates/whisper/src/router/mod.rs"),
            make_target("bar", "crates/whisper/src/router/code.rs"),
            make_target("baz", "crates/whisper/src/lib.rs"),
        ];
        assert_eq!(coherence_score(&targets), 1.0);
    }

    #[test]
    fn test_coherence_score_all_scattered() {
        let targets = vec![
            make_target("a", "crates/whisper/src/router/mod.rs"),
            make_target("b", "crates/core/src/lib.rs"),
            make_target("c", "crates/search/src/indexer.rs"),
            make_target("d", "crates/chronos/src/lib.rs"),
            make_target("e", "crates/husk/src/scanner.rs"),
        ];
        let cs = coherence_score(&targets);
        assert_eq!(cs, 0.0);
    }

    #[test]
    fn test_coherence_score_partial() {
        // 3 targets, 2 modules → CS = 1 - 1/2 = 0.5
        let targets = vec![
            make_target("a", "crates/whisper/src/router/mod.rs"),
            make_target("b", "crates/whisper/src/lib.rs"),
            make_target("c", "crates/core/src/lib.rs"),
        ];
        assert!((coherence_score(&targets) - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_coherence_gate_passes_coherent() {
        let mut targets = vec![
            make_target("foo", "crates/whisper/src/router/mod.rs"),
            make_target("bar", "crates/whisper/src/router/code.rs"),
            make_target("baz", "crates/whisper/src/lib.rs"),
        ];
        let original_len = targets.len();
        coherence_gate(&mut targets, ModelTier::Galactic);
        assert_eq!(targets.len(), original_len); // No pruning
    }

    #[test]
    fn test_coherence_gate_prunes_scattered() {
        let mut targets = vec![
            make_target("a", "crates/whisper/src/router/mod.rs"),
            make_target("b", "crates/whisper/src/lib.rs"),
            make_target("c", "crates/core/src/lib.rs"),
            make_target("d", "crates/search/src/indexer.rs"),
            make_target("e", "crates/chronos/src/lib.rs"),
            make_target("f", "crates/husk/src/scanner.rs"),
        ];
        // CS = 1 - 4/5 = 0.2 < 0.4 → gate triggers
        // Atomic: keep top 2 clusters (whisper has 2, rest have 1 each)
        coherence_gate(&mut targets, ModelTier::Atomic);
        assert!(targets.len() < 6);
        // Largest cluster (whisper) should be first
        assert!(targets[0]
            .file_path
            .as_deref()
            .unwrap()
            .starts_with("crates/whisper"));
    }

    #[test]
    fn test_coherence_gate_skips_small_sets() {
        let mut targets = vec![
            make_target("a", "crates/whisper/src/mod.rs"),
            make_target("b", "crates/core/src/lib.rs"),
        ];
        let original = targets.clone();
        coherence_gate(&mut targets, ModelTier::Galactic);
        // ≤2 targets → no action
        assert_eq!(targets.len(), original.len());
    }

    #[test]
    fn test_coherence_score_single_target() {
        let targets = vec![make_target("a", "crates/core/src/lib.rs")];
        assert_eq!(coherence_score(&targets), 1.0);
    }

    #[test]
    fn test_coherence_score_empty() {
        let targets: Vec<Target> = vec![];
        assert_eq!(coherence_score(&targets), 1.0);
    }
}
