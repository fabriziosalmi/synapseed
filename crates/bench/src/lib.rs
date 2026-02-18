//! # synapseed-bench — Benchmark Engine for Reproducible SCR Evaluation
//!
//! Runs JSONL question suites against the SYNAPSEED `ask` orchestrator,
//! measuring Semantic Compression Ratio (SCR), F1, precision, recall,
//! SID correlation, and hallucination rate.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────┐        ┌──────────────┐       ┌─────────────────┐
//! │ JSONL Suite  │──load──│ BenchEngine  │──ask──│ synapseed_whisper│
//! │ (questions)  │        │  (scoring)   │       │   ask_raw()     │
//! └─────────────┘        └──────┬───────┘       └─────────────────┘
//!                               │
//!                        ┌──────▼───────┐
//!                        │ BenchReport  │
//!                        │  (JSON out)  │
//!                        └──────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,no_run
//! use synapseed_bench::run_benchmark;
//! use synapseed_core::context::SynapseContext;
//!
//! // ctx must have all plugins initialized (cortex, chronos, husk, search…)
//! # fn example(ctx: &SynapseContext) -> anyhow::Result<()> {
//! let report = run_benchmark("suites/grounding_v1.jsonl", ctx)?;
//! println!("Aggregate F1: {:.3}", report.aggregate.mean_f1);
//! # Ok(())
//! # }
//! ```

mod report;
mod scoring;
mod suite;

pub use report::{AggregateMetrics, BenchmarkReport, QuestionResult, ReportMetadata};
pub use scoring::{score_response, QuestionScore};
pub use suite::{BenchQuestion, Difficulty, QuestionCategory};

use anyhow::{Context, Result};
use std::time::Instant;
use synapseed_core::context::SynapseContext;
use tracing::{info, warn};

/// Run a full benchmark suite against the `ask` orchestrator.
///
/// Loads questions from a JSONL file, invokes `ask_raw` for each,
/// scores responses against ground truth, and returns a structured report.
pub fn run_benchmark(suite_path: &str, ctx: &SynapseContext) -> Result<BenchmarkReport> {
    let resolved = resolve_suite_path(suite_path, ctx);
    let questions = suite::load_suite(&resolved)
        .with_context(|| format!("Failed to load suite from '{resolved}'"))?;

    info!(
        suite = %resolved,
        questions = questions.len(),
        "Starting benchmark run"
    );

    let mut results = Vec::with_capacity(questions.len());
    let mut total_hallucinated_files = 0usize;
    let mut total_questions = 0usize;

    for question in &questions {
        total_questions += 1;
        info!(id = %question.id, "Running question {}/{}", total_questions, questions.len());

        // Call ask_raw directly — zero JSON-RPC overhead
        let q_start = Instant::now();
        let whisper_result = synapseed_whisper::router::ask_raw(&question.question, ctx, false);
        let latency_ms = q_start.elapsed().as_secs_f64() * 1000.0;

        // Score the response against ground truth
        let score = scoring::score_response(question, &whisper_result, ctx);

        let response_tokens = estimate_tokens(&whisper_result.smart_context);
        let scr = if response_tokens > 0 {
            score.facts_matched as f64 / response_tokens as f64 * 1000.0
        } else {
            0.0
        };

        let result = QuestionResult {
            id: question.id.clone(),
            question: question.question.clone(),
            difficulty: question.difficulty,
            category: question.category.clone(),
            sid: whisper_result.sid,
            targets_found: whisper_result.targets.len(),
            facts_matched: score.facts_matched,
            facts_total: score.facts_total,
            precision: score.precision,
            recall: score.recall,
            f1: score.f1,
            hallucinated_files: score.hallucinated_files.clone(),
            response_tokens,
            scr,
            latency_ms,
            bottleneck: whisper_result.pipeline_metrics.bottleneck().to_string(),
        };

        total_hallucinated_files += result.hallucinated_files.len();

        if result.f1 < 0.5 {
            warn!(
                id = %result.id,
                f1 = result.f1,
                sid = result.sid,
                "Low-scoring question"
            );
        }

        results.push(result);
    }

    let aggregate = compute_aggregate(&results, total_hallucinated_files, total_questions);

    let project_root = ctx.project_root().display().to_string();
    let version = env!("CARGO_PKG_VERSION").to_string();

    let metadata = ReportMetadata {
        suite_path: resolved,
        question_count: total_questions,
        project_root,
        version,
        timestamp: now_iso8601(),
    };

    let report = BenchmarkReport {
        metadata,
        questions: results,
        aggregate,
    };

    info!(
        f1 = report.aggregate.mean_f1,
        scr = report.aggregate.mean_scr,
        hallucination_rate = report.aggregate.hallucination_rate,
        mean_latency_ms = format!("{:.1}", report.aggregate.mean_latency_ms).as_str(),
        p95_latency_ms = format!("{:.1}", report.aggregate.p95_latency_ms).as_str(),
        "Benchmark complete"
    );

    Ok(report)
}

/// Resolve a suite path: if relative, try project_root/suites/ first,
/// then project_root directly.
fn resolve_suite_path(suite_path: &str, ctx: &SynapseContext) -> String {
    let path = std::path::Path::new(suite_path);
    if path.is_absolute() && path.exists() {
        return suite_path.to_string();
    }
    // Try under project_root
    let project_root = ctx.project_root();
    let candidate = project_root.join(suite_path);
    if candidate.exists() {
        return candidate.display().to_string();
    }
    // Try under project_root/suites/
    let candidate = project_root.join("suites").join(suite_path);
    if candidate.exists() {
        return candidate.display().to_string();
    }
    // Fall back to as-is
    suite_path.to_string()
}

/// Estimate token count (~4 chars per token, standard approximation).
fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

/// ISO 8601 timestamp without external deps.
fn now_iso8601() -> String {
    use std::time::SystemTime;

    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    // Simple UTC breakdown
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Days since epoch to Y-M-D (simplified Gregorian)
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Epoch is 1970-01-01
    let mut year = 1970u64;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let month_days: &[u64] = if is_leap(year) {
        &[31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        &[31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 0u64;
    for &md in month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    (year, month + 1, days + 1)
}

fn is_leap(y: u64) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

/// Compute aggregate metrics across all question results.
fn compute_aggregate(
    results: &[QuestionResult],
    total_hallucinated_files: usize,
    total_questions: usize,
) -> AggregateMetrics {
    if results.is_empty() {
        return AggregateMetrics::default();
    }
    let n = results.len() as f64;

    let mean_f1: f64 = results.iter().map(|r| r.f1).sum::<f64>() / n;
    let mean_precision: f64 = results.iter().map(|r| r.precision).sum::<f64>() / n;
    let mean_recall: f64 = results.iter().map(|r| r.recall).sum::<f64>() / n;
    let mean_sid: f64 = results.iter().map(|r| r.sid).sum::<f64>() / n;
    let mean_scr: f64 = results.iter().map(|r| r.scr).sum::<f64>() / n;

    let hallucination_rate = if total_questions > 0 {
        total_hallucinated_files as f64 / total_questions as f64
    } else {
        0.0
    };

    // SID-F1 Pearson correlation
    let sid_f1_correlation = pearson_correlation(
        &results.iter().map(|r| r.sid).collect::<Vec<_>>(),
        &results.iter().map(|r| r.f1).collect::<Vec<_>>(),
    );

    // Difficulty breakdown
    let easy: Vec<_> = results
        .iter()
        .filter(|r| r.difficulty == Difficulty::Easy)
        .collect();
    let medium: Vec<_> = results
        .iter()
        .filter(|r| r.difficulty == Difficulty::Medium)
        .collect();
    let hard: Vec<_> = results
        .iter()
        .filter(|r| r.difficulty == Difficulty::Hard)
        .collect();

    // Latency stats
    let mean_latency_ms = results.iter().map(|r| r.latency_ms).sum::<f64>() / n;
    let mut latencies: Vec<f64> = results.iter().map(|r| r.latency_ms).collect();
    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p95_idx = ((latencies.len() as f64 * 0.95).ceil() as usize).min(latencies.len()) - 1;
    let p95_latency_ms = latencies[p95_idx];
    let min_latency_ms = latencies.first().copied().unwrap_or(0.0);
    let max_latency_ms = latencies.last().copied().unwrap_or(0.0);

    AggregateMetrics {
        mean_f1,
        mean_precision,
        mean_recall,
        mean_sid,
        mean_scr,
        hallucination_rate,
        sid_f1_correlation,
        questions_total: total_questions,
        perfect_scores: results
            .iter()
            .filter(|r| r.f1 >= 1.0 - f64::EPSILON)
            .count(),
        zero_scores: results.iter().filter(|r| r.f1 < f64::EPSILON).count(),
        easy_mean_f1: mean_f1_of(&easy),
        medium_mean_f1: mean_f1_of(&medium),
        hard_mean_f1: mean_f1_of(&hard),
        mean_latency_ms,
        p95_latency_ms,
        min_latency_ms,
        max_latency_ms,
    }
}

fn mean_f1_of(results: &[&QuestionResult]) -> f64 {
    if results.is_empty() {
        return 0.0;
    }
    results.iter().map(|r| r.f1).sum::<f64>() / results.len() as f64
}

/// Pearson correlation coefficient.
fn pearson_correlation(xs: &[f64], ys: &[f64]) -> f64 {
    let n = xs.len() as f64;
    if n < 2.0 {
        return 0.0;
    }
    let mean_x = xs.iter().sum::<f64>() / n;
    let mean_y = ys.iter().sum::<f64>() / n;

    let mut cov = 0.0;
    let mut var_x = 0.0;
    let mut var_y = 0.0;
    for (x, y) in xs.iter().zip(ys.iter()) {
        let dx = x - mean_x;
        let dy = y - mean_y;
        cov += dx * dy;
        var_x += dx * dx;
        var_y += dy * dy;
    }
    let denom = (var_x * var_y).sqrt();
    if denom < f64::EPSILON {
        0.0
    } else {
        cov / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcdefgh"), 2);
    }

    #[test]
    fn test_pearson_correlation_perfect() {
        let xs = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let ys = vec![2.0, 4.0, 6.0, 8.0, 10.0];
        let r = pearson_correlation(&xs, &ys);
        assert!((r - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_pearson_correlation_negative() {
        let xs = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let ys = vec![10.0, 8.0, 6.0, 4.0, 2.0];
        let r = pearson_correlation(&xs, &ys);
        assert!((r + 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_pearson_correlation_zero() {
        let xs = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let ys = vec![5.0, 5.0, 5.0, 5.0, 5.0];
        let r = pearson_correlation(&xs, &ys);
        assert!(r.abs() < 1e-10);
    }

    #[test]
    fn test_now_iso8601_format() {
        let ts = now_iso8601();
        // YYYY-MM-DDTHH:MM:SSZ
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.as_bytes()[4], b'-');
        assert_eq!(ts.as_bytes()[10], b'T');
    }

    #[test]
    fn test_days_to_ymd_epoch() {
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
    }

    #[test]
    fn test_days_to_ymd_known_date() {
        // 2026-02-14 = 20498 days since epoch
        // Let's verify: 56 years (1970-2025) + 45 days into 2026
        // Not exact but the function should handle it
        let (y, m, d) = days_to_ymd(20498);
        assert_eq!(y, 2026);
        assert_eq!(m, 2);
        assert_eq!(d, 14);
    }
}
