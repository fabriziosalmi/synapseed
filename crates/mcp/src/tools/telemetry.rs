use synapseed_core::context::SynapseContext;
use synapseed_telemetry_sink::store::SpanStore;

use super::text_result;
use crate::protocol::ToolCallResult;

pub(super) fn tool_reset_telemetry(ctx: &SynapseContext) -> ToolCallResult {
    match ctx.get_extension::<SpanStore>() {
        Some(store) => {
            let stats = store.stats();
            store.reset();
            text_result(format!(
                "Telemetry reset. Cleared {} spans across {} locations.",
                stats.total_spans, stats.unique_locations
            ))
        }
        None => text_result("Telemetry sink not active.".into()),
    }
}
