use synapseed_core::context::SynapseContext;
use synapseed_husk::guard::SecurityGuard;
use synapseed_husk::patterns::CodePatternScanner;
use synapseed_root::sentinel::Sentinel;

use super::{error_result, text_result};
use crate::protocol::ToolCallResult;

pub(super) fn tool_scan_security(args: &serde_json::Value, ctx: &SynapseContext) -> ToolCallResult {
    let content = match args.get("content").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return error_result("Missing required parameter: content".into()),
    };

    let mode = args.get("mode").and_then(|v| v.as_str()).unwrap_or("all");

    let mut output_parts = Vec::new();

    // DLP scan (secrets, tokens, keys)
    if mode == "all" || mode == "dlp" {
        let default_guard;
        let guard: &SecurityGuard = if let Some(g) = ctx.get_extension::<SecurityGuard>() {
            default_guard = g;
            &default_guard
        } else {
            default_guard = std::sync::Arc::new(SecurityGuard::with_defaults());
            &default_guard
        };

        match guard.check(content) {
            Ok(()) => output_parts.push("DLP: CLEAN — No sensitive data detected.".to_string()),
            Err(e) => {
                let sanitized = guard.redact(content);
                output_parts.push(format!("DLP ALERT: {e}\n\nSanitized:\n{sanitized}"));
            }
        }
    }

    // Code pattern scan (SQL injection, XSS, command injection, path traversal)
    let patterns_enabled = ctx.dna().security_patterns.enabled;
    if (mode == "all" || mode == "patterns") && patterns_enabled {
        let dna_categories = &ctx.dna().security_patterns.categories;
        let scanner = CodePatternScanner::from_categories(dna_categories);
        let report = scanner.scan(content);

        if report.findings.is_empty() {
            output_parts.push("Patterns: CLEAN — No security anti-patterns detected.".to_string());
        } else {
            let json = serde_json::to_string_pretty(&report).unwrap_or_default();
            output_parts.push(format!("Patterns: {}\n\n{json}", report.status));
        }
    }

    text_result(output_parts.join("\n\n"))
}

pub(super) fn tool_check_command(args: &serde_json::Value, ctx: &SynapseContext) -> ToolCallResult {
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
