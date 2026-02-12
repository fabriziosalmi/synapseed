use synapseed_gym::{Scenario, Trainer};

use super::{error_result, text_result};
use crate::protocol::ToolCallResult;

pub(super) fn tool_train_code(args: &serde_json::Value) -> ToolCallResult {
    let source = match args.get("source").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return error_result("Missing required parameter: source".into()),
    };
    let tests = args
        .get("tests")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let timeout = args
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(60);

    let fuzz = args
        .get("fuzz")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let adversarial = args
        .get("adversarial")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut scenario = Scenario::new(source)
        .with_timeout(timeout)
        .with_fuzz(fuzz)
        .with_adversarial(adversarial);
    if !tests.is_empty() {
        scenario = scenario.with_tests(tests);
    }

    let trainer = Trainer::new();
    match trainer.evaluate(&scenario) {
        Ok(report) => {
            let score = report.score();
            let json = serde_json::to_string_pretty(&report).unwrap_or_default();

            let fuzz_summary = report.fuzz.as_ref().map_or(String::new(), |f| {
                if f.failures.is_empty() {
                    format!(
                        " | Fuzz: {}/{} passed",
                        f.fuzzed_functions, f.fuzzed_functions
                    )
                } else {
                    format!(
                        " | Fuzz: {} failures in {} functions",
                        f.failures.len(),
                        f.fuzzed_functions
                    )
                }
            });

            let adversarial_summary = report.adversarial.as_ref().map_or(String::new(), |a| {
                format!(
                    " | Mutations: {}/{} detected (score: {:.2})",
                    a.detected, a.total_mutations, a.mutation_score
                )
            });

            // Auto-Hardener: generate repro test stubs for survived mutations
            let repro_section = report
                .adversarial
                .as_ref()
                .and_then(|a| a.generate_repro_tests())
                .map(|code| {
                    format!(
                        "\n\n=== AUTO-HARDENER: REPRO TESTS ===\n\
                         Copy these stubs into your test file to harden your suite:\n\n\
                         ```rust\n{code}```"
                    )
                })
                .unwrap_or_default();

            text_result(format!(
                "=== GYM REPORT ===\nScore: {score:.2}/1.00 | Success: {} | Compiled: {} | Warnings: {} | Errors: {}{}{}{}\n\nCompile: {}ms | Binary: {} bytes | Tests: {}ms\n\n{json}{repro_section}",
                report.success,
                report.compilation.compiled,
                report.compilation.warnings,
                report.compilation.errors,
                report.tests.as_ref().map_or(String::new(), |t| format!(" | Tests: {}/{} passed", t.passed, t.total)),
                fuzz_summary,
                adversarial_summary,
                report.metrics.compile_time_ms,
                report.metrics.binary_size_bytes,
                report.metrics.test_time_ms,
            ))
        }
        Err(e) => error_result(format!("Gym evaluation failed: {e}")),
    }
}
