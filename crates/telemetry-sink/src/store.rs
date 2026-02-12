//! Span storage — ring buffer + per-symbol metrics aggregation.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};

use serde::Serialize;

/// Maximum spans kept in the ring buffer.
const MAX_SPANS: usize = 1000;

/// A single OTLP span resolved to source code.
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedSpan {
    pub trace_id: String,
    pub span_id: String,
    pub operation_name: String,
    pub service_name: String,
    pub duration_ms: f64,
    pub file_path: Option<String>,
    pub line_number: Option<u32>,
    pub symbol_name: Option<String>,
    pub timestamp: String,
}

/// Aggregated metrics for a single code location (file:symbol).
#[derive(Debug, Clone, Serialize)]
pub struct SpanMetrics {
    pub key: String,
    pub call_count: u64,
    pub total_duration_ms: f64,
    pub avg_duration_ms: f64,
    pub max_duration_ms: f64,
    pub last_seen: String,
    /// All durations for p95 calculation (kept bounded).
    #[serde(skip)]
    durations: Vec<f64>,
    pub p95_duration_ms: f64,
}

impl SpanMetrics {
    fn new(key: String, duration_ms: f64, timestamp: &str) -> Self {
        Self {
            key,
            call_count: 1,
            total_duration_ms: duration_ms,
            avg_duration_ms: duration_ms,
            max_duration_ms: duration_ms,
            last_seen: timestamp.to_string(),
            durations: vec![duration_ms],
            p95_duration_ms: duration_ms,
        }
    }

    fn update(&mut self, duration_ms: f64, timestamp: &str) {
        self.call_count += 1;
        self.total_duration_ms += duration_ms;
        self.avg_duration_ms = self.total_duration_ms / self.call_count as f64;
        if duration_ms > self.max_duration_ms {
            self.max_duration_ms = duration_ms;
        }
        self.last_seen = timestamp.to_string();

        // Keep at most 200 durations for p95 calculation
        if self.durations.len() >= 200 {
            self.durations.remove(0);
        }
        self.durations.push(duration_ms);
        self.p95_duration_ms = Self::percentile(&self.durations, 95);
    }

    fn percentile(values: &[f64], pct: u8) -> f64 {
        if values.is_empty() {
            return 0.0;
        }
        let mut sorted = values.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let idx = ((pct as f64 / 100.0) * (sorted.len() - 1) as f64).ceil() as usize;
        sorted[idx.min(sorted.len() - 1)]
    }
}

/// Summary statistics for the store.
#[derive(Debug, Clone, Serialize)]
pub struct StoreStats {
    pub total_spans: usize,
    pub unique_locations: usize,
    pub buffer_usage: f64,
}

/// Thread-safe span store with ring buffer and metrics aggregation.
#[derive(Clone)]
pub struct SpanStore {
    inner: Arc<RwLock<StoreInner>>,
}

struct StoreInner {
    spans: VecDeque<ResolvedSpan>,
    metrics: HashMap<String, SpanMetrics>,
}

impl SpanStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(StoreInner {
                spans: VecDeque::with_capacity(MAX_SPANS),
                metrics: HashMap::new(),
            })),
        }
    }

    /// Push a resolved span into the ring buffer and update metrics.
    pub fn push(&self, span: ResolvedSpan) {
        let mut inner = self.inner.write().unwrap();

        // Update per-location metrics
        if let Some(ref file) = span.file_path {
            let key = match &span.symbol_name {
                Some(sym) => format!("{file}:{sym}"),
                None => file.clone(),
            };

            inner
                .metrics
                .entry(key.clone())
                .and_modify(|m| m.update(span.duration_ms, &span.timestamp))
                .or_insert_with(|| SpanMetrics::new(key, span.duration_ms, &span.timestamp));
        }

        // Ring buffer eviction
        if inner.spans.len() >= MAX_SPANS {
            inner.spans.pop_front();
        }
        inner.spans.push_back(span);
    }

    /// Get the N most recent spans.
    pub fn recent(&self, n: usize) -> Vec<ResolvedSpan> {
        let inner = self.inner.read().unwrap();
        inner.spans.iter().rev().take(n).cloned().collect()
    }

    /// Get hotspots sorted by average duration (descending).
    pub fn hotspots(&self) -> Vec<SpanMetrics> {
        let inner = self.inner.read().unwrap();
        let mut spots: Vec<SpanMetrics> = inner.metrics.values().cloned().collect();
        spots.sort_by(|a, b| {
            b.avg_duration_ms
                .partial_cmp(&a.avg_duration_ms)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        spots
    }

    /// Clear all spans and metrics.
    pub fn reset(&self) {
        let mut inner = self.inner.write().unwrap();
        inner.spans.clear();
        inner.metrics.clear();
    }

    /// Get store statistics.
    pub fn stats(&self) -> StoreStats {
        let inner = self.inner.read().unwrap();
        StoreStats {
            total_spans: inner.spans.len(),
            unique_locations: inner.metrics.len(),
            buffer_usage: inner.spans.len() as f64 / MAX_SPANS as f64,
        }
    }
}

impl Default for SpanStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_recent() {
        let store = SpanStore::new();

        for i in 0..5 {
            store.push(ResolvedSpan {
                trace_id: format!("trace-{i}"),
                span_id: format!("span-{i}"),
                operation_name: format!("op-{i}"),
                service_name: "test-svc".into(),
                duration_ms: (i + 1) as f64 * 10.0,
                file_path: Some("src/main.rs".into()),
                line_number: Some(10),
                symbol_name: Some("main".into()),
                timestamp: "2024-01-01T00:00:00Z".into(),
            });
        }

        let recent = store.recent(3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].trace_id, "trace-4"); // most recent first
    }

    #[test]
    fn test_ring_buffer_eviction() {
        let store = SpanStore::new();

        for i in 0..1005 {
            store.push(ResolvedSpan {
                trace_id: format!("trace-{i}"),
                span_id: format!("span-{i}"),
                operation_name: "op".into(),
                service_name: "svc".into(),
                duration_ms: 1.0,
                file_path: None,
                line_number: None,
                symbol_name: None,
                timestamp: "2024-01-01T00:00:00Z".into(),
            });
        }

        let stats = store.stats();
        assert_eq!(stats.total_spans, 1000);
        assert!((stats.buffer_usage - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_hotspots_sorted() {
        let store = SpanStore::new();

        // Fast function
        for _ in 0..3 {
            store.push(ResolvedSpan {
                trace_id: "t1".into(),
                span_id: "s1".into(),
                operation_name: "fast".into(),
                service_name: "svc".into(),
                duration_ms: 5.0,
                file_path: Some("src/fast.rs".into()),
                line_number: Some(1),
                symbol_name: Some("fast_fn".into()),
                timestamp: "2024-01-01T00:00:00Z".into(),
            });
        }

        // Slow function
        for _ in 0..3 {
            store.push(ResolvedSpan {
                trace_id: "t2".into(),
                span_id: "s2".into(),
                operation_name: "slow".into(),
                service_name: "svc".into(),
                duration_ms: 500.0,
                file_path: Some("src/slow.rs".into()),
                line_number: Some(1),
                symbol_name: Some("slow_fn".into()),
                timestamp: "2024-01-01T00:00:00Z".into(),
            });
        }

        let hotspots = store.hotspots();
        assert_eq!(hotspots.len(), 2);
        assert!(hotspots[0].avg_duration_ms > hotspots[1].avg_duration_ms);
        assert_eq!(hotspots[0].key, "src/slow.rs:slow_fn");
    }

    #[test]
    fn test_reset() {
        let store = SpanStore::new();
        store.push(ResolvedSpan {
            trace_id: "t".into(),
            span_id: "s".into(),
            operation_name: "op".into(),
            service_name: "svc".into(),
            duration_ms: 10.0,
            file_path: Some("a.rs".into()),
            line_number: None,
            symbol_name: None,
            timestamp: "2024-01-01T00:00:00Z".into(),
        });

        assert_eq!(store.stats().total_spans, 1);
        store.reset();
        assert_eq!(store.stats().total_spans, 0);
        assert_eq!(store.stats().unique_locations, 0);
    }
}
