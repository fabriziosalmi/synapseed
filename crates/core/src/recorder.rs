//! # Flight Recorder — Session Memory with Dual-Track Architecture
//!
//! Maintains two parallel registers for the MCP server:
//!
//! ## Via 1: Working Set (High Fidelity, Recent)
//! Circular buffer of the last N concrete events: symbols resolved, files touched,
//! errors encountered. Provides immediate context for "next token prediction".
//!
//! ## Via 2: Journey Map (Compressed, Complete)
//! Sequential list of **Session Phases** representing macro-activities.
//! Uses deterministic stripping: consecutive events in the same module collapse
//! into a single phase node, recording only **context shifts**.
//!
//! ## Design Constraints
//! - **Zero I/O**: Pure in-memory, sub-microsecond per event.
//! - **Passive**: Exposed as MCP Resource only. Never injected into search/ranking.
//! - **Deterministic**: No AI — just path clustering + timestamp arithmetic.
//! - **Lightweight**: ~1KB typical session (symbolic IDs, no source text).
//!
//! ## Integration
//! Registered in `SynapseContext` via `ExtensionRegistry`. Updated from:
//! - MCP tool dispatch (tool invocations → working set + journey map)
//! - Event bus subscriber (FileChanged, SymbolResolved, DiagnosticUpdated)
//!
//! ## MCP Exposure
//! - Resource: `synapseed://session/recorder` — full flight recorder state
//! - Injected into `synapseed://context/active` as `"flight_recorder"` field

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

// ── Configuration ──────────────────────────────────────────────────────

/// Max events in the working set (Via 1).
const WORKING_SET_CAPACITY: usize = 20;

/// Max phases in the journey map (Via 2).
const JOURNEY_MAP_CAPACITY: usize = 50;

/// If no events arrive for this long, the current phase auto-closes.
const PHASE_IDLE_TIMEOUT: Duration = Duration::from_secs(300); // 5 min

/// Minimum events before a loop is detected (same module oscillation).
const LOOP_DETECTION_WINDOW: usize = 6;

// ── Working Set (Via 1) ────────────────────────────────────────────────

/// A concrete event in the working set — high-fidelity, recent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkingSetEntry {
    /// Event kind: "tool", "file_change", "symbol_resolved", "diagnostic", "error".
    pub kind: EventKind,
    /// Primary identifier (tool name, file path, symbol name).
    pub subject: String,
    /// Secondary detail (tool args summary, change kind, error message).
    pub detail: Option<String>,
    /// Module path extracted from subject (e.g., "crates/whisper" from "crates/whisper/src/router/mod.rs").
    pub module: String,
    /// Monotonic timestamp (seconds since recorder creation).
    #[serde(skip)]
    pub instant: Option<Instant>,
    /// Wall-clock offset in seconds from session start (for serialization).
    pub offset_secs: u64,
}

/// Typed event kinds tracked by the working set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// MCP tool invocation (ask, search, scan, etc.)
    ToolCall,
    /// File change detected (created, modified, deleted)
    FileChange,
    /// Symbol resolved by AST engine
    SymbolResolved,
    /// Compiler diagnostic (error or warning)
    Diagnostic,
    /// Search query executed
    SearchQuery,
}

// ── Journey Map (Via 2) ────────────────────────────────────────────────

/// A compressed session phase — represents a macro-activity in one module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JourneyPhase {
    /// Phase sequence number (1-based).
    pub seq: usize,
    /// Primary module being worked on (e.g., "crates/husk").
    pub module: String,
    /// Detected activity pattern.
    pub activity: Activity,
    /// Number of raw events collapsed into this phase.
    pub event_count: usize,
    /// Distinct files touched in this phase.
    pub files_touched: Vec<String>,
    /// Duration of this phase.
    pub duration_secs: u64,
    /// Offset from session start when this phase began.
    pub started_at_offset: u64,
    /// Index of the phase that triggered this one (if detected).
    pub triggered_by: Option<usize>,
    /// Whether this phase is still active (open-ended).
    pub active: bool,
}

/// Activity type inferred from event patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Activity {
    /// Read-heavy: search, lookup, hoist, ask dominate.
    Exploring,
    /// Write-heavy: file changes, diagnostics dominate.
    Coding,
    /// Fix-heavy: diagnostics + quickfix + repeated file changes.
    Debugging,
    /// Security/QA: scan, architect, janitor dominate.
    Hardening,
    /// Mixed or insufficient data to classify.
    Mixed,
}

impl std::fmt::Display for Activity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Activity::Exploring => write!(f, "Exploring"),
            Activity::Coding => write!(f, "Coding"),
            Activity::Debugging => write!(f, "Debugging"),
            Activity::Hardening => write!(f, "Hardening"),
            Activity::Mixed => write!(f, "Mixed"),
        }
    }
}

// ── Loop Detection ─────────────────────────────────────────────────────

/// Detected loop pattern in the journey map.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopAlert {
    /// Modules involved in the oscillation.
    pub modules: Vec<String>,
    /// Number of back-and-forth switches detected.
    pub oscillations: usize,
    /// Suggestion for the user.
    pub hint: String,
}

// ── Flight Recorder ────────────────────────────────────────────────────

/// The Flight Recorder — dual-track session memory.
///
/// Thread-safe: wrap in `Arc<parking_lot::Mutex<FlightRecorder>>` for shared access.
pub struct FlightRecorder {
    /// Via 1: circular buffer of recent concrete events.
    working_set: VecDeque<WorkingSetEntry>,
    /// Via 2: compressed timeline of session phases.
    journey: Vec<JourneyPhase>,
    /// Monotonic clock origin for timestamp calculations.
    epoch: Instant,
    /// Phase sequence counter.
    next_seq: usize,
    /// Dependency hints: maps module → modules it imports (pre-populated from AST).
    dep_hints: Vec<(String, String)>,
    /// Event counters for activity classification in current phase.
    current_counters: ActivityCounters,
}

#[derive(Debug, Default)]
struct ActivityCounters {
    reads: u32,     // search, lookup, hoist, ask
    writes: u32,    // file_change (Modified/Created)
    fixes: u32,     // diagnostics, quickfix
    security: u32,  // scan, architect, janitor
}

impl ActivityCounters {
    fn classify(&self) -> Activity {
        let total = self.reads + self.writes + self.fixes + self.security;
        if total == 0 {
            return Activity::Mixed;
        }
        // Debugging: fixes > 40% of activity
        if self.fixes > 0 && (self.fixes * 100 / total) >= 40 {
            return Activity::Debugging;
        }
        // Hardening: security > 40%
        if self.security > 0 && (self.security * 100 / total) >= 40 {
            return Activity::Hardening;
        }
        // Coding: writes > 40%
        if self.writes > 0 && (self.writes * 100 / total) >= 40 {
            return Activity::Coding;
        }
        // Exploring: reads > 40%
        if self.reads > 0 && (self.reads * 100 / total) >= 40 {
            return Activity::Exploring;
        }
        Activity::Mixed
    }

    fn reset(&mut self) {
        *self = Self::default();
    }

    fn record(&mut self, kind: EventKind, tool_name: Option<&str>) {
        match kind {
            EventKind::SearchQuery | EventKind::SymbolResolved => self.reads += 1,
            EventKind::FileChange => self.writes += 1,
            EventKind::Diagnostic => self.fixes += 1,
            EventKind::ToolCall => {
                if let Some(name) = tool_name {
                    match name {
                        "search" | "lookup" | "hoist" | "ask" | "similar" | "consult" | "blame" => {
                            self.reads += 1
                        }
                        "diagnostics" | "quickfix" | "check" => self.fixes += 1,
                        "scan" | "architect" | "janitor" | "janitor-fix" | "diagnose" | "oracle" => {
                            self.security += 1
                        }
                        _ => {} // Neutral tools don't count
                    }
                }
            }
        }
    }
}

impl FlightRecorder {
    /// Create an empty flight recorder.
    pub fn new() -> Self {
        Self {
            working_set: VecDeque::with_capacity(WORKING_SET_CAPACITY),
            journey: Vec::with_capacity(JOURNEY_MAP_CAPACITY / 2),
            epoch: Instant::now(),
            next_seq: 1,
            dep_hints: Vec::new(),
            current_counters: ActivityCounters::default(),
        }
    }

    /// Populate dependency hints from the code graph.
    /// Called once after indexing. Format: ("crates/mcp", "crates/core").
    pub fn set_dep_hints(&mut self, deps: Vec<(String, String)>) {
        self.dep_hints = deps;
    }

    /// Record an event. This is the main entry point.
    ///
    /// - Pushes to working set (Via 1).
    /// - Updates or creates a journey phase (Via 2).
    pub fn record(&mut self, kind: EventKind, subject: &str, detail: Option<&str>, tool_name: Option<&str>) {
        let now = Instant::now();
        let offset = now.duration_since(self.epoch).as_secs();
        let module = extract_module(subject);

        // ── Via 1: Working Set ─────────────────────────────────────
        let entry = WorkingSetEntry {
            kind,
            subject: subject.to_string(),
            detail: detail.map(|s| truncate_detail(s, 120)),
            module: module.clone(),
            instant: Some(now),
            offset_secs: offset,
        };

        if self.working_set.len() >= WORKING_SET_CAPACITY {
            self.working_set.pop_front();
        }
        self.working_set.push_back(entry);

        // ── Via 2: Journey Map (context shift detection) ───────────
        self.current_counters.record(kind, tool_name);

        let should_new_phase = if let Some(current) = self.journey.last() {
            // Different module → context shift
            if current.module != module && !module.is_empty() {
                true
            }
            // Same module but idle timeout → auto-close and reopen
            else if offset.saturating_sub(current.started_at_offset + current.duration_secs)
                > PHASE_IDLE_TIMEOUT.as_secs()
            {
                true
            } else {
                false
            }
        } else {
            // No phases yet → start first one
            !module.is_empty()
        };

        if should_new_phase && !module.is_empty() {
            // Close current phase
            if let Some(current) = self.journey.last_mut() {
                current.active = false;
                current.duration_secs = offset.saturating_sub(current.started_at_offset);
                current.activity = self.current_counters.classify();
            }

            // Detect causal link: does the new module depend on the old?
            let triggered_by = self.journey.last().and_then(|prev| {
                let from = &prev.module;
                let to = &module;
                // Check if new module imports old module (dependency arrow)
                if self.dep_hints.iter().any(|(a, b)| a == to && b == from) {
                    Some(prev.seq)
                } else {
                    None
                }
            });

            // Evict oldest if at capacity
            if self.journey.len() >= JOURNEY_MAP_CAPACITY {
                self.journey.remove(0);
            }

            let seq = self.next_seq;
            self.next_seq += 1;

            self.journey.push(JourneyPhase {
                seq,
                module: module.clone(),
                activity: Activity::Mixed,
                event_count: 1,
                files_touched: if kind == EventKind::FileChange {
                    vec![subject.to_string()]
                } else {
                    Vec::new()
                },
                duration_secs: 0,
                started_at_offset: offset,
                triggered_by,
                active: true,
            });

            self.current_counters.reset();
            self.current_counters.record(kind, tool_name);
        } else if let Some(current) = self.journey.last_mut() {
            // Extend current phase
            current.event_count += 1;
            current.duration_secs = offset.saturating_sub(current.started_at_offset);
            current.activity = self.current_counters.classify();

            if kind == EventKind::FileChange
                && !current.files_touched.contains(&subject.to_string())
            {
                if current.files_touched.len() < 10 {
                    current.files_touched.push(subject.to_string());
                }
            }
        }
    }

    // ── Queries ────────────────────────────────────────────────────

    /// Get the working set (Via 1).
    pub fn working_set(&self) -> &VecDeque<WorkingSetEntry> {
        &self.working_set
    }

    /// Get the journey map (Via 2).
    pub fn journey(&self) -> &[JourneyPhase] {
        &self.journey
    }

    /// Total events recorded.
    pub fn total_events(&self) -> usize {
        self.journey.iter().map(|p| p.event_count).sum::<usize>()
    }

    /// Current active phase (if any).
    pub fn current_phase(&self) -> Option<&JourneyPhase> {
        self.journey.last().filter(|p| p.active)
    }

    /// Detect if we're in a loop (oscillating between the same modules).
    pub fn detect_loop(&self) -> Option<LoopAlert> {
        if self.journey.len() < LOOP_DETECTION_WINDOW {
            return None;
        }

        let recent: Vec<&str> = self
            .journey
            .iter()
            .rev()
            .take(LOOP_DETECTION_WINDOW)
            .map(|p| p.module.as_str())
            .collect();

        // Check for A-B-A-B pattern (oscillation between 2 modules)
        if recent.len() >= 4 {
            let a = recent[0];
            let b = recent[1];
            if a != b
                && recent.iter().enumerate().all(|(i, m)| {
                    if i % 2 == 0 { *m == a } else { *m == b }
                })
            {
                let oscillations = recent.len() / 2;
                return Some(LoopAlert {
                    modules: vec![a.to_string(), b.to_string()],
                    oscillations,
                    hint: format!(
                        "Detected {oscillations} back-and-forth switches between [{a}] and [{b}]. \
                         Consider addressing the root cause in one module before switching."
                    ),
                });
            }
        }

        // Check for same-module repeated phases (fix→error→fix→error)
        let same_module: Vec<_> = recent
            .iter()
            .filter(|m| **m == recent[0])
            .collect();
        if same_module.len() >= 3 {
            return Some(LoopAlert {
                modules: vec![recent[0].to_string()],
                oscillations: same_module.len(),
                hint: format!(
                    "You've returned to [{}] {} times recently. Consider a different approach or rollback.",
                    recent[0],
                    same_module.len()
                ),
            });
        }

        None
    }

    /// Render a compact markdown summary for model consumption.
    /// This is the "prompt injection" format from the design doc.
    pub fn render_markdown(&self) -> String {
        let mut out = String::with_capacity(1024);
        out.push_str("# SESSION FLIGHT RECORDER\n");

        if self.journey.is_empty() {
            out.push_str("No activity recorded yet.\n");
            return out;
        }

        // Journey map (compressed phases)
        out.push_str("## JOURNEY\n");
        for phase in &self.journey {
            let status = if phase.active { "NOW" } else { "Done" };
            let trigger = phase
                .triggered_by
                .map(|t| format!(" (caused by Phase {t})"))
                .unwrap_or_default();
            out.push_str(&format!(
                "- Phase {} [{status}]: `{}` — {} ({} events, {}s){trigger}\n",
                phase.seq,
                phase.module,
                phase.activity,
                phase.event_count,
                phase.duration_secs,
            ));
        }

        // Loop detection
        if let Some(alert) = self.detect_loop() {
            out.push_str("\n## WARNING: LOOP DETECTED\n");
            out.push_str(&alert.hint);
            out.push('\n');
        }

        // Working set (last 5 entries for conciseness)
        out.push_str("\n## ACTIVE CONTEXT (Last 5)\n");
        for entry in self.working_set.iter().rev().take(5) {
            let detail = entry
                .detail
                .as_deref()
                .map(|d| format!(" — {d}"))
                .unwrap_or_default();
            out.push_str(&format!(
                "- [{:?}] `{}`{detail}\n",
                entry.kind, entry.subject
            ));
        }

        out
    }

    /// Serialize to JSON for the MCP resource.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "total_events": self.total_events(),
            "phase_count": self.journey.len(),
            "working_set_size": self.working_set.len(),
            "current_phase": self.current_phase(),
            "journey": self.journey,
            "working_set": self.working_set.iter().collect::<Vec<_>>(),
            "loop_alert": self.detect_loop(),
        })
    }
}

impl Default for FlightRecorder {
    fn default() -> Self {
        Self::new()
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

/// Extract module path from a file path or subject string.
/// "crates/whisper/src/router/mod.rs" → "crates/whisper"
/// "benchmark/search/run.py" → "benchmark/search"
/// "Cargo.toml" → "root"
/// "search" (tool name) → "" (tools don't shift phases directly)
fn extract_module(path: &str) -> String {
    // If it looks like a file path (contains / or .)
    if path.contains('/') {
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() >= 2 {
            // "crates/X/..." → "crates/X"
            // "bin/X/..." → "bin/X"
            // "benchmark/X/..." → "benchmark/X"
            return format!("{}/{}", parts[0], parts[1]);
        }
    }
    // Root-level files
    if path.contains('.') {
        return "root".to_string();
    }
    // Tool names or symbols — no module context
    String::new()
}

fn truncate_detail(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..s.floor_char_boundary(max)])
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_module() {
        assert_eq!(extract_module("crates/whisper/src/router/mod.rs"), "crates/whisper");
        assert_eq!(extract_module("bin/synapseed/src/main.rs"), "bin/synapseed");
        assert_eq!(extract_module("benchmark/search/run.py"), "benchmark/search");
        assert_eq!(extract_module("Cargo.toml"), "root");
        assert_eq!(extract_module("search"), "");
    }

    #[test]
    fn test_working_set_circular_buffer() {
        let mut rec = FlightRecorder::new();
        for i in 0..(WORKING_SET_CAPACITY + 5) {
            rec.record(
                EventKind::ToolCall,
                &format!("crates/mod{}/src/lib.rs", i % 3),
                Some("search"),
                Some("search"),
            );
        }
        assert_eq!(rec.working_set.len(), WORKING_SET_CAPACITY);
    }

    #[test]
    fn test_journey_phase_creation() {
        let mut rec = FlightRecorder::new();
        // Events in same module → single phase
        rec.record(EventKind::ToolCall, "crates/husk/src/scanner.rs", None, Some("search"));
        rec.record(EventKind::ToolCall, "crates/husk/src/patterns.rs", None, Some("lookup"));
        rec.record(EventKind::FileChange, "crates/husk/src/scanner.rs", Some("Modified"), None);

        assert_eq!(rec.journey.len(), 1);
        assert_eq!(rec.journey[0].module, "crates/husk");
        assert_eq!(rec.journey[0].event_count, 3);
        assert!(rec.journey[0].active);
    }

    #[test]
    fn test_context_shift_creates_new_phase() {
        let mut rec = FlightRecorder::new();
        rec.record(EventKind::ToolCall, "crates/husk/src/scanner.rs", None, Some("search"));
        rec.record(EventKind::ToolCall, "crates/whisper/src/router/mod.rs", None, Some("ask"));

        assert_eq!(rec.journey.len(), 2);
        assert_eq!(rec.journey[0].module, "crates/husk");
        assert!(!rec.journey[0].active);
        assert_eq!(rec.journey[1].module, "crates/whisper");
        assert!(rec.journey[1].active);
    }

    #[test]
    fn test_activity_classification() {
        let mut counters = ActivityCounters::default();
        counters.reads = 10;
        counters.writes = 1;
        assert_eq!(counters.classify(), Activity::Exploring);

        counters.reset();
        counters.writes = 10;
        counters.reads = 1;
        assert_eq!(counters.classify(), Activity::Coding);

        counters.reset();
        counters.fixes = 10;
        counters.reads = 2;
        assert_eq!(counters.classify(), Activity::Debugging);

        counters.reset();
        counters.security = 10;
        assert_eq!(counters.classify(), Activity::Hardening);
    }

    #[test]
    fn test_loop_detection_oscillation() {
        let mut rec = FlightRecorder::new();
        // Create A-B-A-B-A-B pattern
        for _ in 0..3 {
            rec.record(EventKind::FileChange, "crates/auth/src/lib.rs", Some("Modified"), None);
            rec.record(EventKind::FileChange, "crates/tests/src/lib.rs", Some("Modified"), None);
        }
        let alert = rec.detect_loop();
        assert!(alert.is_some(), "Should detect oscillation");
        let alert = alert.unwrap();
        assert_eq!(alert.modules.len(), 2);
        assert!(alert.hint.contains("back-and-forth"));
    }

    #[test]
    fn test_no_loop_on_normal_flow() {
        let mut rec = FlightRecorder::new();
        rec.record(EventKind::ToolCall, "crates/core/src/lib.rs", None, Some("search"));
        rec.record(EventKind::ToolCall, "crates/husk/src/lib.rs", None, Some("search"));
        rec.record(EventKind::ToolCall, "crates/mcp/src/lib.rs", None, Some("search"));
        assert!(rec.detect_loop().is_none());
    }

    #[test]
    fn test_render_markdown_not_empty() {
        let mut rec = FlightRecorder::new();
        rec.record(EventKind::ToolCall, "crates/whisper/src/router/mod.rs", None, Some("ask"));
        rec.record(EventKind::FileChange, "crates/whisper/src/router/extraction.rs", Some("Modified"), None);
        let md = rec.render_markdown();
        assert!(md.contains("FLIGHT RECORDER"));
        assert!(md.contains("crates/whisper"));
        assert!(md.contains("ACTIVE CONTEXT"));
    }

    #[test]
    fn test_to_json_structure() {
        let mut rec = FlightRecorder::new();
        rec.record(EventKind::ToolCall, "crates/core/src/lib.rs", None, Some("search"));
        let json = rec.to_json();
        assert!(json.get("total_events").is_some());
        assert!(json.get("journey").is_some());
        assert!(json.get("working_set").is_some());
    }

    #[test]
    fn test_triggered_by_dependency() {
        let mut rec = FlightRecorder::new();
        // mcp depends on core
        rec.set_dep_hints(vec![
            ("crates/mcp".to_string(), "crates/core".to_string()),
        ]);
        rec.record(EventKind::FileChange, "crates/core/src/lib.rs", Some("Modified"), None);
        rec.record(EventKind::FileChange, "crates/mcp/src/lib.rs", Some("Modified"), None);

        assert_eq!(rec.journey.len(), 2);
        assert_eq!(rec.journey[1].triggered_by, Some(1)); // Phase 2 triggered by Phase 1
    }

    #[test]
    fn test_empty_recorder() {
        let rec = FlightRecorder::new();
        assert_eq!(rec.total_events(), 0);
        assert!(rec.current_phase().is_none());
        assert!(rec.detect_loop().is_none());
        let md = rec.render_markdown();
        assert!(md.contains("No activity"));
    }

    #[test]
    fn test_journey_capacity_eviction() {
        let mut rec = FlightRecorder::new();
        for i in 0..(JOURNEY_MAP_CAPACITY + 5) {
            rec.record(
                EventKind::FileChange,
                &format!("crates/mod{}/src/lib.rs", i),
                Some("Modified"),
                None,
            );
        }
        assert!(rec.journey.len() <= JOURNEY_MAP_CAPACITY);
    }
}
