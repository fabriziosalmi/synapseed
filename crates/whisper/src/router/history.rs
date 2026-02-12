use synapseed_chronos::historian::Historian;
use synapseed_core::context::SynapseContext;

use super::{HistoryContext, Intent, Target};

pub(super) fn gather_history(
    intent: &Intent,
    targets: &[Target],
    ctx: &SynapseContext,
) -> Option<HistoryContext> {
    if !matches!(
        intent,
        Intent::BugFix | Intent::Explain | Intent::Refactor | Intent::General
    ) {
        return None;
    }

    let historian = ctx.get_extension::<Historian>()?;

    // Analyze the first target file
    let target = targets.iter().find(|t| t.file_path.is_some())?;
    let file_path = target.file_path.as_deref()?;

    let analysis = historian.analyze_history(file_path, None, None).ok()?;

    let recent_commits: Vec<serde_json::Value> = analysis
        .commits
        .iter()
        .take(5)
        .map(|c| serde_json::to_value(c).unwrap_or_default())
        .collect();

    Some(HistoryContext {
        file: file_path.to_string(),
        total_commits: analysis.total_commits,
        hotspot_score: analysis.hotspot_score,
        risk: analysis.semantic_summary.risk_indicator.clone(),
        recent_commits,
        top_authors: analysis.top_authors.clone(),
        convergence_rate: analysis.convergence_rate,
        rigidity: analysis.rigidity,
        fix_chain_count: analysis.fix_chain_count,
    })
}
