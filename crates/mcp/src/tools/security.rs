use synapseed_core::context::SynapseContext;
use synapseed_husk::guard::SecurityGuard;
use synapseed_root::sentinel::Sentinel;

use super::{error_result, text_result};
use crate::protocol::ToolCallResult;

pub(super) fn tool_scan_security(
    args: &serde_json::Value,
    ctx: &SynapseContext,
) -> ToolCallResult {
    let content = match args.get("content").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return error_result("Missing required parameter: content".into()),
    };

    // Try shared guard from HuskPlugin, fallback to defaults
    let default_guard;
    let guard: &SecurityGuard = if let Some(g) = ctx.get_extension::<SecurityGuard>() {
        default_guard = g;
        &default_guard
    } else {
        default_guard = std::sync::Arc::new(SecurityGuard::with_defaults());
        &default_guard
    };

    match guard.check(content) {
        Ok(()) => text_result("CLEAN: No sensitive data detected.".into()),
        Err(e) => {
            let sanitized = guard.redact(content);
            text_result(format!("ALERT: {e}\n\nSanitized:\n{sanitized}"))
        }
    }
}

pub(super) fn tool_check_command(
    args: &serde_json::Value,
    ctx: &SynapseContext,
) -> ToolCallResult {
    let command = match args.get("command").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return error_result("Missing required parameter: command".into()),
    };

    // Try shared sentinel from RootPlugin
    if let Some(sentinel) = ctx.get_extension::<Sentinel>() {
        return match sentinel.evaluate(command) {
            Ok(action) => text_result(format!("ALLOWED ({action:?}): {command}")),
            Err(e) => text_result(format!("DENIED: {e}")),
        };
    }

    // Fallback
    let sentinel = match Sentinel::with_defaults() {
        Ok(s) => s,
        Err(e) => return error_result(format!("Failed to create sentinel: {e}")),
    };

    match sentinel.evaluate(command) {
        Ok(action) => text_result(format!("ALLOWED ({action:?}): {command}")),
        Err(e) => text_result(format!("DENIED: {e}")),
    }
}
