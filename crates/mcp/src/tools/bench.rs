//! MCP tool: run_benchmark — Benchmark Engine for reproducible SCR evaluation.

use synapseed_bench::{run_benchmark, BenchmarkReport};
use synapseed_core::context::SynapseContext;
use synapseed_core::error::safe_resolve_path;

use super::{error_result, text_result};
use crate::protocol::ToolCallResult;

/// Run a benchmark suite against the `ask` orchestrator.
///
/// Loads a JSONL question suite, invokes `ask` for each question via direct
/// Rust API (zero JSON-RPC overhead), scores responses against ground truth,
/// and returns a structured report with F1, SCR, SID correlation, and
/// hallucination metrics.
pub(super) fn tool_run_benchmark(args: &serde_json::Value, ctx: &SynapseContext) -> ToolCallResult {
    let suite_path = match args.get("suite_path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return error_result("Missing required parameter: suite_path".into()),
    };

    let format = args
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("summary");

    // Security: validate path is within project
    let root = ctx.project_root();
    if safe_resolve_path(&root, suite_path).is_err() {
        // Try bare filename under suites/ before rejecting
        let suites_candidate = format!("crates/bench/suites/{suite_path}");
        if safe_resolve_path(&root, &suites_candidate).is_err() {
            return error_result(format!(
                "Path traversal blocked: '{suite_path}' is outside the project root"
            ));
        }
    }

    match run_benchmark(suite_path, ctx) {
        Ok(report) => format_report(&report, format),
        Err(e) => error_result(format!("Benchmark failed: {e:#}")),
    }
}

fn format_report(report: &BenchmarkReport, format: &str) -> ToolCallResult {
    match format {
        "json" => text_result(report.to_json()),
        _ => {
            let mut out = report.summary();
            out.push_str("\n---\n\n<details><summary>Full JSON report</summary>\n\n```json\n");
            out.push_str(&report.to_json());
            out.push_str("\n```\n\n</details>\n");
            text_result(out)
        }
    }
}
