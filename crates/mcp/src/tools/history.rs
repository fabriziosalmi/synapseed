use synapseed_core::context::SynapseContext;
use synapseed_search::indexer::SemanticIndex;

use super::{error_result, get_historian, text_result};
use crate::protocol::ToolCallResult;

pub(super) fn tool_git_history(
    args: &serde_json::Value,
    ctx: &SynapseContext,
) -> ToolCallResult {
    let file = match args.get("file").and_then(|v| v.as_str()) {
        Some(f) => f,
        None => return error_result("Missing required parameter: file".into()),
    };
    let start = args.get("start_line").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
    let end = args.get("end_line").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

    let historian = match get_historian(ctx) {
        Ok(h) => h,
        Err(e) => return e,
    };

    match historian.blame_lines(file, start, end) {
        Ok(blame) => {
            if blame.is_empty() {
                text_result(format!("No blame data for {file}:{start}-{end}"))
            } else {
                let json = serde_json::to_string_pretty(&blame).unwrap_or_default();
                text_result(format!("Blame for {file}:{start}-{end}:\n{json}"))
            }
        }
        // D46: Socratic error — help the LLM course-correct instead of raw pass-through.
        Err(e) => {
            let msg = format!("{e}");
            let hint = if msg.contains("does not exist") || msg.contains("not found") {
                format!(
                    "Blame failed for '{file}': the file does not exist in Git history.\n\n\
                     Suggestions:\n\
                     - Verify the file path with `verify_path`\n\
                     - The file may be new/untracked — try `search` to find it by content\n\
                     - Use `hoist` to see the full project structure"
                )
            } else if msg.contains("no such path") || msg.contains("path '") {
                format!(
                    "Blame failed for '{file}': the path was not found in the current HEAD.\n\n\
                     Suggestions:\n\
                     - The file may have been renamed or deleted — try `intent` to check recent commits\n\
                     - Use `search` with the old filename to find where it moved"
                )
            } else if msg.contains("not a git repository") || msg.contains("Failed to open git repo") {
                "Blame failed: this project is not a Git repository.\n\n\
                 Suggestions:\n\
                 - Use `search` or `lookup` for code intelligence without Git\n\
                 - Use `hoist` to explore the project structure".to_string()
            } else {
                format!("Blame failed for '{file}': {msg}")
            };
            error_result(hint)
        }
    }
}

pub(super) fn tool_analyze_history(
    args: &serde_json::Value,
    ctx: &SynapseContext,
) -> ToolCallResult {
    let file = match args.get("file").and_then(|v| v.as_str()) {
        Some(f) => f,
        None => return error_result("Missing required parameter: file".into()),
    };
    let start_line = args
        .get("start_line")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);
    let end_line = args
        .get("end_line")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    let historian = match get_historian(ctx) {
        Ok(h) => h,
        Err(e) => return e,
    };

    match historian.analyze_history(file, start_line, end_line) {
        Ok(analysis) => {
            let json = serde_json::to_string_pretty(&analysis).unwrap_or_default();
            let range_str = match analysis.line_range {
                Some((s, e)) => format!(":{s}-{e}"),
                None => String::new(),
            };

            // D55: Churn×PageRank junction — flag tech debt candidates.
            // High churn + low structural importance = likely dead-weight code.
            let pagerank_hint = if analysis.hotspot_score > 40.0 {
                ctx.get_extension::<SemanticIndex>()
                    .and_then(|idx| idx.get_pagerank_score(file))
                    .filter(|&pr| pr < 0.02)
                    .map(|pr| {
                        format!(
                            "\n⚠ TECHNICAL DEBT CANDIDATE: High churn ({:.1}) but low PageRank ({:.4}) — \
                             this file changes often yet has minimal structural importance.",
                            analysis.hotspot_score, pr
                        )
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };

            text_result(format!(
                "=== History Analysis: {file}{range_str} ===\n\
                 Commits: {} | Hotspot: {:.1} | Risk: {}{pagerank_hint}\n\n{json}",
                analysis.total_commits,
                analysis.hotspot_score,
                analysis.semantic_summary.risk_indicator,
            ))
        }
        // D46: Socratic error for analyze too.
        Err(e) => {
            let msg = format!("{e}");
            if msg.contains("does not exist") || msg.contains("not found") || msg.contains("no such path") {
                error_result(format!(
                    "History analysis failed for '{file}': file not found in Git history.\n\n\
                     Suggestions:\n\
                     - Verify the path with `verify_path`\n\
                     - Use `search` to find the file by content or symbol name"
                ))
            } else {
                error_result(format!("History analysis failed: {msg}"))
            }
        }
    }
}

pub(super) fn tool_git_intent_summary(
    args: &serde_json::Value,
    ctx: &SynapseContext,
) -> ToolCallResult {
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

    let historian = match get_historian(ctx) {
        Ok(h) => h,
        Err(e) => return e,
    };

    match historian.summarize_intent(limit) {
        Ok(intent) => {
            let json = serde_json::to_string_pretty(&intent).unwrap_or_default();
            text_result(format!("{}\n\n{json}", intent.summary))
        }
        Err(e) => error_result(format!("Intent summary failed: {e}")),
    }
}
