use synapseed_core::context::SynapseContext;
use synapseed_shadow_check::runner::DiagnosticStore;

use super::{error_result, text_result};
use crate::protocol::ToolCallResult;

pub(super) fn tool_get_diagnostics(
    args: &serde_json::Value,
    ctx: &SynapseContext,
) -> ToolCallResult {
    use synapseed_shadow_check::runner::MinSeverity;

    let store = match ctx.get_extension::<DiagnosticStore>() {
        Some(s) => s,
        None => return text_result("Shadow compiler not active (no Cargo.toml found or not initialized). Run `synapseed init` first.".into()),
    };

    let file_filter = args.get("file").and_then(|v| v.as_str());
    let min_severity = args
        .get("min_severity")
        .and_then(|v| v.as_str())
        .map(MinSeverity::from_str_loose)
        .unwrap_or(MinSeverity::Warning);

    let snap = store.filtered_snapshot(min_severity);
    let diagnostics = match file_filter {
        Some(file) => snap
            .diagnostics
            .iter()
            .filter(|d| d.file_path == file || file.ends_with(&d.file_path))
            .cloned()
            .collect(),
        None => snap.diagnostics,
    };

    if diagnostics.is_empty() {
        text_result(format!(
            "CLEAN: No diagnostics at severity {:?}+. Last check took {}ms.",
            min_severity, snap.last_check_ms
        ))
    } else {
        let json = serde_json::to_string_pretty(&diagnostics).unwrap_or_default();
        text_result(format!(
            "{} error(s), {} warning(s) (check took {}ms, filter: {:?}+):\n{json}",
            snap.error_count, snap.warning_count, snap.last_check_ms, min_severity
        ))
    }
}

pub(super) fn tool_apply_quick_fix(
    args: &serde_json::Value,
    ctx: &SynapseContext,
) -> ToolCallResult {
    let file = match args.get("file").and_then(|v| v.as_str()) {
        Some(f) => f,
        None => return error_result("Missing required parameter: file".into()),
    };
    let error_code = match args.get("error_code").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return error_result("Missing required parameter: error_code".into()),
    };

    let store = match ctx.get_extension::<DiagnosticStore>() {
        Some(s) => s,
        None => return error_result("Shadow compiler not active.".into()),
    };

    match store.apply_fix(file, error_code) {
        Ok(msg) => text_result(msg),
        Err(e) => error_result(e),
    }
}
