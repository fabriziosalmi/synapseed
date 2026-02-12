use synapseed_core::context::SynapseContext;

use super::{error_result, text_result};
use crate::protocol::ToolCallResult;

pub(super) fn tool_ask_synapseed(
    args: &serde_json::Value,
    ctx: &SynapseContext,
) -> ToolCallResult {
    let query = match args.get("query").and_then(|v| v.as_str()) {
        Some(q) => q,
        None => return error_result("Missing required parameter: query".into()),
    };

    let result = synapseed_whisper::router::ask(query, ctx);

    let json = serde_json::to_string_pretty(&result).unwrap_or_default();

    // Smart Context Injection: the LLM prompt is enriched with
    // the smart_context summary followed by the full JSON data.
    text_result(format!(
        "{}\n\n--- Full Context ---\n{json}",
        result.smart_context
    ))
}
