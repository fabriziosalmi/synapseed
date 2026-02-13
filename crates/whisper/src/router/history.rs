use std::collections::HashSet;

use synapseed_chronos::historian::Historian;
use synapseed_core::context::SynapseContext;

use super::{HistoryContext, Intent, Target};

/// Gather git history for ALL unique target files (v4.12.0: multi-file).
/// Returns a Vec instead of Option — empty if intent doesn't warrant history.
pub(super) fn gather_histories(
    intent: &Intent,
    targets: &[Target],
    ctx: &SynapseContext,
) -> Vec<HistoryContext> {
    if !matches!(
        intent,
        Intent::BugFix | Intent::Explain | Intent::Refactor | Intent::General
    ) {
        return Vec::new();
    }

    let historian = match ctx.get_extension::<Historian>() {
        Some(h) => h,
        None => return Vec::new(),
    };

    // Collect unique file paths from targets (preserve order, max 5 files)
    let mut seen = HashSet::new();
    let file_paths: Vec<&str> = targets
        .iter()
        .filter_map(|t| t.file_path.as_deref())
        .filter(|fp| seen.insert(*fp))
        .take(5)
        .collect();

    let mut histories = Vec::new();

    for file_path in file_paths {
        let analysis = match historian.analyze_history(file_path, None, None) {
            Ok(a) => a,
            Err(_) => continue,
        };

        let recent_commits: Vec<serde_json::Value> = analysis
            .commits
            .iter()
            .take(5)
            .map(|c| serde_json::to_value(c).unwrap_or_default())
            .collect();

        histories.push(HistoryContext {
            file: file_path.to_string(),
            total_commits: analysis.total_commits,
            hotspot_score: analysis.hotspot_score,
            risk: analysis.semantic_summary.risk_indicator.clone(),
            recent_commits,
            top_authors: analysis.top_authors.clone(),
            convergence_rate: analysis.convergence_rate,
            rigidity: analysis.rigidity,
            fix_chain_count: analysis.fix_chain_count,
        });
    }

    histories
}
