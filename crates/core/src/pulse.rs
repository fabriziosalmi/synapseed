//! # Pulse — Exponential Decay Activity Counters
//!
//! A lightweight in-memory system that tracks file and symbol access
//! patterns with time-weighted relevance.  Every touch is stored in a
//! ring buffer; querying applies **exponential decay** so recent events
//! dominate while old ones fade naturally.
//!
//! ```text
//! weight = count_at_age × e^(−λ × age_seconds)
//! λ = ln(2) / half_life       (default half_life = 480 s ≈ 8 min)
//! ```
//!
//! ## Design Principles
//! 1. **Never impactful** — Pulse data is advisory.  Consumers use it
//!    as an optional tiebreaker, never as a hard gate.
//! 2. **Zero-cost when idle** — Ring buffers only allocate on first
//!    `record()`; reading an empty counter returns an empty vec.
//! 3. **Transparent** — Exposed as MCP resource `synapseed://pulse`.
//! 4. **Ephemeral** — In-RAM only, dies with the session.

use std::collections::HashMap;
use std::time::Instant;

// ── Constants ────────────────────────────────────────────────────────

/// Default half-life in seconds.  After this time, an event's weight
/// is halved.  8 minutes matches the typical "focus window" of a
/// developer editing code before switching context.
const DEFAULT_HALF_LIFE_SECS: f64 = 480.0;

/// Maximum events stored per counter key.  Old events are evicted
/// FIFO when this limit is reached.  1024 is ~16 KB of memory.
const MAX_EVENTS_PER_KEY: usize = 1024;

/// Decay constant λ = ln(2) / half_life.
const LAMBDA: f64 = std::f64::consts::LN_2 / DEFAULT_HALF_LIFE_SECS;

// ── Public Types ─────────────────────────────────────────────────────

/// A single recorded event: the value (e.g. a file path) and when it
/// happened.
#[derive(Debug, Clone)]
struct PulseEvent {
    value: String,
    timestamp: Instant,
}

/// A value with its time-weighted score.
#[derive(Debug, Clone)]
pub struct PulseEntry {
    pub value: String,
    pub score: f64,
    pub raw_count: usize,
}

/// Thread-safe activity counter registry.
///
/// Internally uses a `parking_lot::RwLock` for low-contention
/// concurrent access.  Writers (`record`) take an exclusive lock
/// briefly; readers (`weighted_top`) use a shared lock.
#[derive(Debug)]
pub struct PulseStore {
    counters: parking_lot::RwLock<HashMap<String, Vec<PulseEvent>>>,
    half_life: f64,
    lambda: f64,
}

impl Default for PulseStore {
    fn default() -> Self {
        Self::new()
    }
}

impl PulseStore {
    /// Create a new store with the default 8-minute half-life.
    pub fn new() -> Self {
        Self {
            counters: parking_lot::RwLock::new(HashMap::new()),
            half_life: DEFAULT_HALF_LIFE_SECS,
            lambda: LAMBDA,
        }
    }

    /// Create a store with a custom half-life (in seconds).
    #[cfg(test)]
    pub fn with_half_life(half_life_secs: f64) -> Self {
        let lambda = std::f64::consts::LN_2 / half_life_secs;
        Self {
            counters: parking_lot::RwLock::new(HashMap::new()),
            half_life: half_life_secs,
            lambda,
        }
    }

    /// Record an event.  O(1) amortized — appends to the ring buffer
    /// for the given counter key.
    ///
    /// # Arguments
    /// * `counter` — Counter name, e.g. `"file_touched"` or `"symbol_touched"`.
    /// * `value` — The thing being counted, e.g. a relative file path.
    pub fn record(&self, counter: &str, value: impl Into<String>) {
        let event = PulseEvent {
            value: value.into(),
            timestamp: Instant::now(),
        };
        let mut map = self.counters.write();
        let events = map.entry(counter.to_string()).or_default();
        events.push(event);
        // Evict oldest if over capacity
        if events.len() > MAX_EVENTS_PER_KEY {
            events.drain(..events.len() - MAX_EVENTS_PER_KEY);
        }
    }

    /// Record an event with a specific timestamp (for testing).
    #[cfg(test)]
    pub fn record_at(&self, counter: &str, value: impl Into<String>, at: Instant) {
        let event = PulseEvent {
            value: value.into(),
            timestamp: at,
        };
        let mut map = self.counters.write();
        let events = map.entry(counter.to_string()).or_default();
        events.push(event);
        if events.len() > MAX_EVENTS_PER_KEY {
            events.drain(..events.len() - MAX_EVENTS_PER_KEY);
        }
    }

    /// Get the top `n` values for a counter, ranked by time-weighted score.
    ///
    /// Each occurrence of a value contributes `e^(−λ × age)` to its
    /// total score.  A file touched 5 times 3 minutes ago scores much
    /// higher than one touched once 12 minutes ago.
    ///
    /// Returns an empty vec if the counter has no events.
    pub fn weighted_top(&self, counter: &str, n: usize) -> Vec<PulseEntry> {
        let map = self.counters.read();
        let events = match map.get(counter) {
            Some(e) => e,
            None => return Vec::new(),
        };

        let now = Instant::now();
        let mut scores: HashMap<&str, (f64, usize)> = HashMap::new();

        for event in events {
            let age = now.duration_since(event.timestamp).as_secs_f64();
            let weight = (-self.lambda * age).exp();
            let entry = scores.entry(&event.value).or_insert((0.0, 0));
            entry.0 += weight;
            entry.1 += 1;
        }

        let mut ranked: Vec<PulseEntry> = scores
            .into_iter()
            .map(|(value, (score, count))| PulseEntry {
                value: value.to_string(),
                score,
                raw_count: count,
            })
            .collect();

        ranked.sort_by(|a, b| b.score.total_cmp(&a.score));
        ranked.truncate(n);
        ranked
    }

    /// Get the weighted score for a specific value in a counter.
    /// Returns 0.0 if not found.
    pub fn score_of(&self, counter: &str, value: &str) -> f64 {
        let map = self.counters.read();
        let events = match map.get(counter) {
            Some(e) => e,
            None => return 0.0,
        };

        let now = Instant::now();
        let mut total = 0.0;
        for event in events {
            if event.value == value {
                let age = now.duration_since(event.timestamp).as_secs_f64();
                total += (-self.lambda * age).exp();
            }
        }
        total
    }

    /// Total number of distinct values across all counters.
    pub fn distinct_values(&self) -> usize {
        let map = self.counters.read();
        let mut seen = std::collections::HashSet::new();
        for events in map.values() {
            for event in events {
                seen.insert(&event.value);
            }
        }
        seen.len()
    }

    /// Total number of events across all counters.
    pub fn total_events(&self) -> usize {
        let map = self.counters.read();
        map.values().map(|v| v.len()).sum()
    }

    /// Number of registered counter keys.
    pub fn counter_count(&self) -> usize {
        self.counters.read().len()
    }

    /// Reset all counters, discarding accumulated history.
    ///
    /// Use on context pivot (e.g., switching project or starting a fresh session)
    /// so stale frequency data doesn't bias new queries (D26).
    pub fn reset(&self) {
        self.counters.write().clear();
    }

    /// The configured half-life in seconds.
    pub fn half_life(&self) -> f64 {
        self.half_life
    }

    /// Snapshot of all counters for serialization (MCP resource).
    pub fn snapshot(&self) -> PulseSnapshot {
        let map = self.counters.read();
        let now = Instant::now();

        let mut counters = Vec::new();
        for (name, events) in map.iter() {
            let mut value_scores: HashMap<&str, (f64, usize)> = HashMap::new();
            for event in events {
                let age = now.duration_since(event.timestamp).as_secs_f64();
                let weight = (-self.lambda * age).exp();
                let entry = value_scores.entry(&event.value).or_insert((0.0, 0));
                entry.0 += weight;
                entry.1 += 1;
            }

            let mut entries: Vec<PulseEntry> = value_scores
                .into_iter()
                .map(|(v, (s, c))| PulseEntry {
                    value: v.to_string(),
                    score: s,
                    raw_count: c,
                })
                .collect();
            entries.sort_by(|a, b| b.score.total_cmp(&a.score));

            counters.push(CounterSnapshot {
                name: name.clone(),
                total_events: events.len(),
                top: entries.into_iter().take(10).collect(),
            });
        }
        counters.sort_by(|a, b| a.name.cmp(&b.name));

        PulseSnapshot {
            half_life_secs: self.half_life,
            total_events: self.total_events(),
            counters,
        }
    }
}

/// Serializable snapshot of the full Pulse state.
#[derive(Debug, Clone)]
pub struct PulseSnapshot {
    pub half_life_secs: f64,
    pub total_events: usize,
    pub counters: Vec<CounterSnapshot>,
}

/// Snapshot of a single counter.
#[derive(Debug, Clone)]
pub struct CounterSnapshot {
    pub name: String,
    pub total_events: usize,
    pub top: Vec<PulseEntry>,
}

// ── Well-Known Counter Names ─────────────────────────────────────────

/// Files that appear in query results (ask, lookup, search, blame).
pub const COUNTER_FILE_TOUCHED: &str = "file_touched";
/// Symbols that appear in query results.
pub const COUNTER_SYMBOL_TOUCHED: &str = "symbol_touched";

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn empty_store_returns_empty() {
        let store = PulseStore::new();
        assert!(store.weighted_top("nope", 5).is_empty());
        assert_eq!(store.score_of("nope", "any"), 0.0);
        assert_eq!(store.total_events(), 0);
    }

    #[test]
    fn record_and_retrieve() {
        let store = PulseStore::new();
        store.record("file_touched", "src/main.rs");
        store.record("file_touched", "src/main.rs");
        store.record("file_touched", "src/lib.rs");

        let top = store.weighted_top("file_touched", 5);
        assert_eq!(top.len(), 2);
        // main.rs was touched twice, should be first
        assert_eq!(top[0].value, "src/main.rs");
        assert_eq!(top[0].raw_count, 2);
        assert_eq!(top[1].value, "src/lib.rs");
        assert_eq!(top[1].raw_count, 1);
        // Recent events should have score close to their count
        assert!(top[0].score > 1.9); // 2 × ~1.0
        assert!(top[1].score > 0.9); // 1 × ~1.0
    }

    #[test]
    fn decay_reduces_old_events() {
        let store = PulseStore::with_half_life(10.0); // 10s half-life for fast test
        let now = Instant::now();

        // Old event: 20 seconds ago (2 half-lives → weight ≈ 0.25)
        store.record_at("f", "old.rs", now - Duration::from_secs(20));
        // Recent event: just now (weight ≈ 1.0)
        store.record_at("f", "new.rs", now);

        let top = store.weighted_top("f", 5);
        assert_eq!(top[0].value, "new.rs");
        assert!(top[0].score > top[1].score);
        // Old event should be roughly 1/4 weight
        assert!(top[1].score < 0.30);
    }

    #[test]
    fn frequency_beats_recency_within_window() {
        let store = PulseStore::with_half_life(60.0); // 1 min half-life
        let now = Instant::now();

        // File A: touched 5 times, 30 seconds ago
        for _ in 0..5 {
            store.record_at("f", "a.rs", now - Duration::from_secs(30));
        }
        // File B: touched 1 time, just now
        store.record_at("f", "b.rs", now);

        let top = store.weighted_top("f", 5);
        // A has higher total weighted score despite being older
        assert_eq!(top[0].value, "a.rs");
        assert!(top[0].score > top[1].score);
    }

    #[test]
    fn ring_buffer_evicts_oldest() {
        let store = PulseStore::new();
        for i in 0..2000 {
            store.record("x", format!("file_{}.rs", i % 100));
        }
        // Should have at most MAX_EVENTS_PER_KEY events
        assert!(store.total_events() <= MAX_EVENTS_PER_KEY);
    }

    #[test]
    fn score_of_specific_value() {
        let store = PulseStore::new();
        store.record("file_touched", "src/main.rs");
        store.record("file_touched", "src/lib.rs");

        let score = store.score_of("file_touched", "src/main.rs");
        assert!(score > 0.9);
        let miss = store.score_of("file_touched", "nope.rs");
        assert_eq!(miss, 0.0);
    }

    #[test]
    fn snapshot_is_consistent() {
        let store = PulseStore::new();
        store.record("file_touched", "a.rs");
        store.record("file_touched", "b.rs");
        store.record("symbol_touched", "main");

        let snap = store.snapshot();
        assert_eq!(snap.total_events, 3);
        assert_eq!(snap.counters.len(), 2);
        assert_eq!(snap.half_life_secs, DEFAULT_HALF_LIFE_SECS);
    }

    #[test]
    fn counter_isolation() {
        let store = PulseStore::new();
        store.record("file_touched", "a.rs");
        store.record("symbol_touched", "main");

        assert_eq!(store.weighted_top("file_touched", 5).len(), 1);
        assert_eq!(store.weighted_top("symbol_touched", 5).len(), 1);
        assert!(store.weighted_top("other", 5).is_empty());
    }

    #[test]
    fn truncate_to_n() {
        let store = PulseStore::new();
        for i in 0..20 {
            store.record("f", format!("file_{i}.rs"));
        }
        let top3 = store.weighted_top("f", 3);
        assert_eq!(top3.len(), 3);
    }
}
