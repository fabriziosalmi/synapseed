use synapseed_core::context::SynapseContext;

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
        Err(e) => error_result(format!("Blame failed: {e}")),
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
            text_result(format!(
                "=== History Analysis: {file}{range_str} ===\n\
                 Commits: {} | Hotspot: {:.1} | Risk: {}\n\n{json}",
                analysis.total_commits,
                analysis.hotspot_score,
                analysis.semantic_summary.risk_indicator,
            ))
        }
        Err(e) => error_result(format!("History analysis failed: {e}")),
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
