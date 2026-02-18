//! Pipeline Metrics — deterministic per-stage timing for the `ask` pipeline.
//!
//! Every stage of the Whisperer pipeline is timed with `Instant::now()` +
//! `elapsed()`. Results are returned in `WhisperResult.pipeline_metrics` so
//! that:
//!
//! 1. The bench engine can track latency per-question alongside quality metrics
//! 2. MCP resources can surface stage-level bottlenecks
//! 3. Regression tests can assert that no stage regresses by more than X%
//!
//! All durations are in **microseconds** (µs) for sub-millisecond precision
//! without floating-point noise.
//!
//! # Stage Map
//!
//! ```text
//! ┌────────────────┐
//! │ 1. momentum    │ Read ModelTier + SessionPhase + git staged detection
//! ├────────────────┤
//! │ 2. classify    │ Intent classification (keyword heuristics)
//! ├────────────────┤
//! │ 3. extract     │ 5-pass target extraction (search, cortex, explicit refs)
//! ├────────────────┤
//! │ 4. prune       │ Vendor filtering + dedup + truncate to tier budget
//! ├────────────────┤
//! │ 5. coherence   │ Coherence Gate: scatter detection + module clustering
//! ├────────────────┤
//! │ 6. gather      │ Diagnostics + histories + code context + security (parallel-ready)
//! ├────────────────┤
//! │ 7. raw_inject  │ Raw source injection for semantic ballast
//! ├────────────────┤
//! │ 8. session     │ Pulse recording + Cognitive Ledger (FlightRecorder → MomentClassifier)
//! ├────────────────┤
//! │ 9. context     │ Smart context assembly (tier-aware formatting)
//! ├────────────────┤
//! │ 10. finalize   │ SID computation + result struct assembly
//! └────────────────┘
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use serde::Serialize;

/// Per-stage timing for one `ask` pipeline execution.
///
/// All durations in **microseconds** (µs).
#[derive(Debug, Clone, Serialize, Default)]
pub struct PipelineMetrics {
    /// Stage 1: Read ModelTier + SessionPhase + git staged detection
    pub momentum_us: u64,
    /// Stage 2: Intent classification (all intents scored)
    pub classify_us: u64,
    /// Stage 3: 5-pass target extraction
    pub extract_us: u64,
    /// Stage 4: Vendor filtering + dedup + truncate
    pub prune_us: u64,
    /// Stage 5: Coherence Gate
    pub coherence_us: u64,
    /// Stage 6: Gather all context (diagnostics + histories + code + security)
    pub gather_us: u64,
    /// Stage 7: Raw source injection
    pub raw_inject_us: u64,
    /// Stage 8: Pulse recording + Cognitive Ledger
    pub session_us: u64,
    /// Stage 9: Smart context assembly
    pub context_us: u64,
    /// Stage 10: SID computation + result assembly
    pub finalize_us: u64,
    /// Total wall-clock time for the entire pipeline
    pub total_us: u64,
    /// Number of targets after extraction (before pruning)
    pub targets_before_prune: usize,
    /// Number of targets after pruning + coherence
    pub targets_after_prune: usize,
    /// Smart context size in bytes
    pub context_bytes: usize,
    /// Smart context estimated tokens (~4 chars/token)
    pub context_tokens: usize,
}

impl PipelineMetrics {
    /// Returns a human-readable breakdown suitable for logging or MCP output.
    pub fn summary(&self) -> String {
        let stages = [
            ("momentum", self.momentum_us),
            ("classify", self.classify_us),
            ("extract", self.extract_us),
            ("prune", self.prune_us),
            ("coherence", self.coherence_us),
            ("gather", self.gather_us),
            ("raw_inject", self.raw_inject_us),
            ("session", self.session_us),
            ("context", self.context_us),
            ("finalize", self.finalize_us),
        ];

        let mut out = String::with_capacity(512);
        out.push_str(&format!(
            "Pipeline: {:.1}ms total | {} targets → {} after prune | {} tokens\n",
            self.total_us as f64 / 1000.0,
            self.targets_before_prune,
            self.targets_after_prune,
            self.context_tokens,
        ));

        for (name, us) in &stages {
            let pct = if self.total_us > 0 {
                *us as f64 / self.total_us as f64 * 100.0
            } else {
                0.0
            };
            let bar_len = (pct / 2.0).round() as usize;
            let bar: String = "█".repeat(bar_len.min(40));
            out.push_str(&format!(
                "  {:<12} {:>7.1}ms  {:>5.1}% {}\n",
                name,
                *us as f64 / 1000.0,
                pct,
                bar,
            ));
        }

        out
    }

    /// Returns the stage name that consumed the most time.
    pub fn bottleneck(&self) -> &'static str {
        let stages = [
            ("momentum", self.momentum_us),
            ("classify", self.classify_us),
            ("extract", self.extract_us),
            ("prune", self.prune_us),
            ("coherence", self.coherence_us),
            ("gather", self.gather_us),
            ("raw_inject", self.raw_inject_us),
            ("session", self.session_us),
            ("context", self.context_us),
            ("finalize", self.finalize_us),
        ];
        stages
            .iter()
            .max_by_key(|(_, us)| *us)
            .map(|(name, _)| *name)
            .unwrap_or("unknown")
    }

    /// Percentage of total time spent in the given stage.
    pub fn stage_pct(&self, stage_us: u64) -> f64 {
        if self.total_us == 0 {
            0.0
        } else {
            stage_us as f64 / self.total_us as f64 * 100.0
        }
    }
}

// ── Rolling Aggregator (for MCP resource) ──────────────────────────────

/// Thread-safe rolling aggregator that tracks the last N pipeline executions.
/// Used by MCP resources to expose pipeline performance without per-query overhead.
pub struct PipelineAggregator {
    /// Ring buffer of recent metrics
    history: std::sync::Mutex<Vec<PipelineMetrics>>,
    capacity: usize,
    total_queries: AtomicU64,
}

impl PipelineAggregator {
    pub fn new(capacity: usize) -> Self {
        Self {
            history: std::sync::Mutex::new(Vec::with_capacity(capacity)),
            capacity,
            total_queries: AtomicU64::new(0),
        }
    }

    /// Record a completed pipeline run.
    pub fn record(&self, metrics: PipelineMetrics) {
        self.total_queries.fetch_add(1, Ordering::Relaxed);
        let Ok(mut history) = self.history.lock() else {
            return;
        };
        if history.len() >= self.capacity {
            history.remove(0);
        }
        history.push(metrics);
    }

    /// Get the last recorded metrics (most recent query).
    pub fn last(&self) -> Option<PipelineMetrics> {
        self.history.lock().ok().and_then(|h| h.last().cloned())
    }

    /// Total queries processed since startup.
    pub fn total_queries(&self) -> u64 {
        self.total_queries.load(Ordering::Relaxed)
    }

    /// Compute aggregate stats across the rolling window.
    pub fn aggregate(&self) -> AggregateStats {
        let Ok(history) = self.history.lock() else {
            return AggregateStats::default();
        };
        if history.is_empty() {
            return AggregateStats::default();
        }

        let n = history.len() as f64;
        let mut agg = AggregateStats {
            window_size: history.len(),
            total_queries: self.total_queries.load(Ordering::Relaxed),
            ..Default::default()
        };

        for m in history.iter() {
            agg.avg_total_us += m.total_us as f64;
            agg.avg_momentum_us += m.momentum_us as f64;
            agg.avg_classify_us += m.classify_us as f64;
            agg.avg_extract_us += m.extract_us as f64;
            agg.avg_prune_us += m.prune_us as f64;
            agg.avg_coherence_us += m.coherence_us as f64;
            agg.avg_gather_us += m.gather_us as f64;
            agg.avg_raw_inject_us += m.raw_inject_us as f64;
            agg.avg_session_us += m.session_us as f64;
            agg.avg_context_us += m.context_us as f64;
            agg.avg_finalize_us += m.finalize_us as f64;
            agg.avg_context_tokens += m.context_tokens as f64;

            if m.total_us > agg.max_total_us {
                agg.max_total_us = m.total_us;
            }
            if agg.min_total_us == 0 || m.total_us < agg.min_total_us {
                agg.min_total_us = m.total_us;
            }
        }

        agg.avg_total_us /= n;
        agg.avg_momentum_us /= n;
        agg.avg_classify_us /= n;
        agg.avg_extract_us /= n;
        agg.avg_prune_us /= n;
        agg.avg_coherence_us /= n;
        agg.avg_gather_us /= n;
        agg.avg_raw_inject_us /= n;
        agg.avg_session_us /= n;
        agg.avg_context_us /= n;
        agg.avg_finalize_us /= n;
        agg.avg_context_tokens /= n;

        // P95 total latency
        let mut totals: Vec<u64> = history.iter().map(|m| m.total_us).collect();
        totals.sort_unstable();
        let p95_idx = ((totals.len() as f64 * 0.95).ceil() as usize).min(totals.len()) - 1;
        agg.p95_total_us = totals[p95_idx];

        agg
    }
}

/// Aggregate statistics across the rolling window.
#[derive(Debug, Clone, Serialize, Default)]
pub struct AggregateStats {
    pub window_size: usize,
    pub total_queries: u64,
    pub avg_total_us: f64,
    pub min_total_us: u64,
    pub max_total_us: u64,
    pub p95_total_us: u64,
    pub avg_momentum_us: f64,
    pub avg_classify_us: f64,
    pub avg_extract_us: f64,
    pub avg_prune_us: f64,
    pub avg_coherence_us: f64,
    pub avg_gather_us: f64,
    pub avg_raw_inject_us: f64,
    pub avg_session_us: f64,
    pub avg_context_us: f64,
    pub avg_finalize_us: f64,
    pub avg_context_tokens: f64,
}

/// Convenience: time a block and return (result, microseconds).
#[inline]
pub fn timed<T>(f: impl FnOnce() -> T) -> (T, u64) {
    let start = Instant::now();
    let result = f();
    let elapsed_us = start.elapsed().as_micros() as u64;
    (result, elapsed_us)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_metrics_summary() {
        let m = PipelineMetrics {
            momentum_us: 100,
            classify_us: 50,
            extract_us: 5000,
            prune_us: 200,
            coherence_us: 300,
            gather_us: 10000,
            raw_inject_us: 2000,
            session_us: 500,
            context_us: 3000,
            finalize_us: 50,
            total_us: 21200,
            targets_before_prune: 15,
            targets_after_prune: 8,
            context_bytes: 4000,
            context_tokens: 1000,
        };
        let summary = m.summary();
        assert!(summary.contains("21.2ms total"));
        assert!(summary.contains("gather"));
        assert!(summary.contains("extract"));
    }

    #[test]
    fn test_bottleneck() {
        let m = PipelineMetrics {
            gather_us: 9999,
            extract_us: 5000,
            ..PipelineMetrics::default()
        };
        assert_eq!(m.bottleneck(), "gather");
    }

    #[test]
    fn test_aggregator() {
        let agg = PipelineAggregator::new(10);
        let m1 = PipelineMetrics {
            total_us: 1000,
            extract_us: 500,
            ..PipelineMetrics::default()
        };
        let m2 = PipelineMetrics {
            total_us: 2000,
            extract_us: 1500,
            ..PipelineMetrics::default()
        };

        agg.record(m1);
        agg.record(m2);

        assert_eq!(agg.total_queries(), 2);
        let stats = agg.aggregate();
        assert_eq!(stats.window_size, 2);
        assert!((stats.avg_total_us - 1500.0).abs() < 0.1);
        assert_eq!(stats.max_total_us, 2000);
        assert_eq!(stats.min_total_us, 1000);
    }

    #[test]
    fn test_timed() {
        let (result, us) = timed(|| 42);
        assert_eq!(result, 42);
        // Just ensure it doesn't panic and returns a reasonable value
        assert!(us < 1_000_000); // < 1 second
    }
}
