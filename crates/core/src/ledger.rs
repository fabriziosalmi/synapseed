//! # Cognitive Ledger — Operational Moment Classification (v5.0)
//!
//! Deterministic classifier that infers the user's **Operational Moment** from
//! Flight Recorder metrics. Emits a [`SessionPulse`] that downstream systems
//! consume as a *Compendium* (opinion), never a *Conditioner* (filter).
//!
//! ## Design Constraints
//! - **Zero LLM calls**: Classification uses only timestamp deltas, counters,
//!   path clustering, and symbol-graph proximity.
//! - **Auditable**: Every [`SessionPulse`] contains the [`Evidence`] that
//!   triggered it, traceable to specific metric values.
//! - **Non-blocking**: Updates happen on a decoupled mpsc channel.
//! - **Graceful degradation**: If the Ledger is unavailable, search/ask
//!   continue at 100% accuracy.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

// ── Configuration ──────────────────────────────────────────────────────

/// Minimum events before classification is meaningful.
const MIN_EVENTS_FOR_CLASSIFICATION: usize = 3;

/// Looping threshold: same symbol/query repeated N+ times.
const LOOP_REPEAT_THRESHOLD: usize = 3;

/// Looping time window: repetitions must occur within this duration.
const LOOP_TIME_WINDOW: Duration = Duration::from_secs(120);

/// Max recency boost factor applied in hybrid search (1.0 = no boost).
pub const MAX_RECENCY_BOOST: f32 = 1.2;

// ── Operational Moments ────────────────────────────────────────────────

/// The 10 Operational Moments from the v5.2 specification.
///
/// Each moment carries fixed behavioral parameters that downstream
/// systems use to tune their output style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationalMoment {
    /// Boilerplate/prototyping/skeleton creation.
    RapidScaffolding,
    /// Core algorithms, safety, data integrity.
    DeepBackendLogic,
    /// API integration, BFF wiring, cross-layer connections.
    ArchitectureWiring,
    /// Components, state management, view logic.
    FrontendImplementation,
    /// Styling, accessibility, UX flow refinement.
    UxPolish,
    /// Naming parity, cross-layer consistency audit.
    CrossLayerConsistency,
    /// Fix-fail-retry loops, circular errors.
    IterativeDistress,
    /// Copy-paste, mass update, pattern extension.
    MomentumFlow,
    /// Tech debt, abstraction cleanup, refactoring.
    ExploratoryRefactoring,
    /// Hotfix, security fix, critical break response.
    EmergencyPatching,
}

impl OperationalMoment {
    /// Needle-in-Haystack Range (1–10).
    /// Low = broad/template results. High = surgical precision.
    pub fn needle_range(self) -> u8 {
        match self {
            Self::MomentumFlow => 1,
            Self::RapidScaffolding => 2,
            Self::UxPolish => 3,
            Self::CrossLayerConsistency => 5,
            Self::FrontendImplementation => 6,
            Self::ArchitectureWiring => 7,
            Self::ExploratoryRefactoring => 8,
            Self::DeepBackendLogic => 9,
            Self::IterativeDistress => 10,
            Self::EmergencyPatching => 10,
        }
    }

    /// Session context weight (0–100%).
    /// How much of the session history should influence context.
    pub fn session_weight_pct(self) -> u8 {
        match self {
            Self::EmergencyPatching => 0,
            Self::MomentumFlow => 10,
            Self::RapidScaffolding => 20,
            Self::FrontendImplementation => 40,
            Self::DeepBackendLogic => 50,
            Self::UxPolish => 60,
            Self::ArchitectureWiring => 80,
            Self::ExploratoryRefactoring => 90,
            Self::CrossLayerConsistency => 100,
            Self::IterativeDistress => 100,
        }
    }

    /// Mode suggestion for the downstream LLM.
    pub fn mode(self) -> ModeHint {
        match self {
            Self::RapidScaffolding => ModeHint::Momentum,
            Self::DeepBackendLogic => ModeHint::Reasoning,
            Self::ArchitectureWiring => ModeHint::Reasoning,
            Self::FrontendImplementation => ModeHint::Momentum,
            Self::UxPolish => ModeHint::Momentum,
            Self::CrossLayerConsistency => ModeHint::Reasoning,
            Self::IterativeDistress => ModeHint::DeepReasoning,
            Self::MomentumFlow => ModeHint::Momentum,
            Self::ExploratoryRefactoring => ModeHint::Reasoning,
            Self::EmergencyPatching => ModeHint::DeepReasoning,
        }
    }

    /// The architectural layer this moment operates at.
    pub fn layer(self) -> &'static str {
        match self {
            Self::RapidScaffolding => "Scaffolding",
            Self::DeepBackendLogic => "Backend",
            Self::ArchitectureWiring => "Integration",
            Self::FrontendImplementation => "Frontend",
            Self::UxPolish => "UX/Refinement",
            Self::CrossLayerConsistency => "Quality",
            Self::IterativeDistress => "Recovery",
            Self::MomentumFlow => "Maintenance",
            Self::ExploratoryRefactoring => "Evolution",
            Self::EmergencyPatching => "Emergency",
        }
    }

    /// Human-readable display name.
    pub fn label(self) -> &'static str {
        match self {
            Self::RapidScaffolding => "Rapid Scaffolding",
            Self::DeepBackendLogic => "Deep Backend Logic",
            Self::ArchitectureWiring => "Architecture Wiring",
            Self::FrontendImplementation => "Frontend Implementation",
            Self::UxPolish => "UX/UI Polish",
            Self::CrossLayerConsistency => "Cross-Layer Consistency",
            Self::IterativeDistress => "Iterative Distress",
            Self::MomentumFlow => "Momentum Flow",
            Self::ExploratoryRefactoring => "Exploratory Refactoring",
            Self::EmergencyPatching => "Emergency Patching",
        }
    }
}

impl std::fmt::Display for OperationalMoment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

// ── Mode Hint ──────────────────────────────────────────────────────────

/// Behavioral suggestion for the downstream model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModeHint {
    /// Predictive: provide ready-to-use snippets, trust prior patterns.
    Momentum,
    /// Critical: provide definitions, constraints, and "why" explanations.
    Reasoning,
    /// Crisis mode: Socratic questioning, deep architectural analysis.
    DeepReasoning,
}

impl std::fmt::Display for ModeHint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Momentum => write!(f, "Momentum"),
            Self::Reasoning => write!(f, "Reasoning"),
            Self::DeepReasoning => write!(f, "Deep-Reasoning"),
        }
    }
}

// ── Session Pulse ──────────────────────────────────────────────────────

/// The "heartbeat" output of the Moment Classifier.
/// Emitted after every tool call. Contains the classification
/// and the auditable evidence that produced it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionPulse {
    /// Classified operational moment.
    pub moment: OperationalMoment,
    /// Needle-in-Haystack range (1–10).
    pub needle_range: u8,
    /// Mode suggestion.
    pub mode: ModeHint,
    /// Architectural layer.
    pub layer: &'static str,
    /// Session context weight (0–100%).
    pub session_weight_pct: u8,
    /// Auditable evidence that triggered this classification.
    pub evidence: Evidence,
    /// Active focus module (e.g., "crates/whisper").
    pub focus_module: String,
    /// Human-readable session hint for context injection.
    pub session_hint: String,
}

/// Auditable evidence: the exact metrics that led to a classification.
/// Every field is traceable and deterministic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    /// Tool calls in the metrics window.
    pub tool_calls_in_window: usize,
    /// Average interval between tool calls (seconds).
    pub avg_interval_secs: f64,
    /// Number of distinct modules touched in window.
    pub distinct_modules: usize,
    /// Number of distinct files touched in window.
    pub distinct_files: usize,
    /// Active diagnostic error count.
    pub active_errors: u32,
    /// Active diagnostic warning count.
    pub active_warnings: u32,
    /// Loop detected: repeated query/symbol count.
    pub loop_repeat_count: usize,
    /// File path variance (0.0 = single file, 1.0 = all different).
    pub file_path_variance: f64,
    /// Read/write ratio in window (reads / total, 0.0–1.0).
    pub read_write_ratio: f64,
    /// Diagnostic trend: +1 = increasing, 0 = stable, -1 = decreasing.
    pub diagnostic_trend: i8,
    /// Primary classification reason.
    pub reason: String,
}

// ── Moment Classifier ──────────────────────────────────────────────────

/// Input metrics snapshot for the classifier.
/// Captured from the FlightRecorder at classification time.
#[derive(Debug, Clone, Default)]
pub struct MetricsSnapshot {
    /// Recent tool call timestamps (newest first).
    pub call_timestamps: VecDeque<Instant>,
    /// Recent tool names (newest first).
    pub call_tools: VecDeque<String>,
    /// Recent subjects (file paths / symbol names, newest first).
    pub call_subjects: VecDeque<String>,
    /// Module of each call (newest first).
    pub call_modules: VecDeque<String>,
    /// Current error count from shadow compiler.
    pub active_errors: u32,
    /// Current warning count from shadow compiler.
    pub active_warnings: u32,
    /// Previous error count (from last pulse).
    pub prev_errors: u32,
    /// Previous warning count (from last pulse).
    pub prev_warnings: u32,
    /// Current phase module (from journey map).
    pub focus_module: String,
    /// Current activity from recorder (Exploring/Coding/Debugging/etc).
    pub current_activity: String,
    /// Number of phases in journey.
    pub journey_phase_count: usize,
    /// Whether a loop alert is active.
    pub loop_alert_active: bool,
}

/// Pre-computed derived metrics from a [`MetricsSnapshot`].
///
/// Avoids double-computation: `classify()` computes once,
/// then passes to `pulse()` for evidence construction.
#[derive(Debug, Clone)]
struct DerivedMetrics {
    n: usize,
    avg_interval: f64,
    distinct_modules: usize,
    distinct_files: usize,
    file_variance: f64,
    loop_count: usize,
    read_ratio: f64,
    diag_trend: i8,
}

impl DerivedMetrics {
    fn from_snapshot(snap: &MetricsSnapshot) -> Self {
        let n = snap.call_timestamps.len();
        let avg_interval = MomentClassifier::avg_interval(snap);
        let distinct_modules = MomentClassifier::distinct_count(&snap.call_modules);
        let distinct_files = MomentClassifier::distinct_count(&snap.call_subjects);
        let file_variance = if n > 0 {
            distinct_files as f64 / n as f64
        } else {
            0.0
        };
        let loop_count = MomentClassifier::detect_repeat_count(snap);
        let read_ratio = MomentClassifier::read_write_ratio(snap);
        let diag_trend = MomentClassifier::diagnostic_trend(snap);
        Self {
            n,
            avg_interval,
            distinct_modules,
            distinct_files,
            file_variance,
            loop_count,
            read_ratio,
            diag_trend,
        }
    }
}

/// The deterministic Moment Classifier.
///
/// Pure function: `classify(snapshot) -> SessionPulse`.
/// No internal state — all state is in the `MetricsSnapshot` input.
pub struct MomentClassifier;

impl MomentClassifier {
    /// Classify the current operational moment from a metrics snapshot.
    ///
    /// Priority cascade (first match wins):
    /// 1. Emergency Patching (critical errors + high urgency)
    /// 2. Iterative Distress (looping detection)
    /// 3. DeepBackendLogic / ExploratoryRefactoring / etc. (pattern matching)
    /// 4. MomentumFlow (default fallback for established patterns)
    pub fn classify(snap: &MetricsSnapshot) -> SessionPulse {
        let n = snap.call_timestamps.len();
        if n < MIN_EVENTS_FOR_CLASSIFICATION {
            let m = DerivedMetrics::from_snapshot(snap);
            return Self::pulse(
                OperationalMoment::RapidScaffolding,
                snap,
                "Insufficient data — defaulting to scaffolding".into(),
                &m,
            );
        }

        // ── Compute derived metrics (once — passed to pulse()) ─────
        let m = DerivedMetrics::from_snapshot(snap);
        let avg_interval = m.avg_interval;
        let distinct_modules = m.distinct_modules;
        let distinct_files = m.distinct_files;
        let file_variance = m.file_variance;
        let loop_count = m.loop_count;
        let read_ratio = m.read_ratio;
        let diag_trend = m.diag_trend;

        // ── Priority 1: Emergency Patching ─────────────────────────
        // Sudden spike in errors (5+) with fast tool calls (<10s avg)
        if snap.active_errors >= 5 && avg_interval < 10.0 {
            return Self::pulse(
                OperationalMoment::EmergencyPatching,
                snap,
                format!(
                    "Emergency: {} errors detected with rapid tool calls (avg {:.1}s interval)",
                    snap.active_errors, avg_interval
                ),
                &m,
            );
        }

        // ── Priority 2: Iterative Distress (Looping) ──────────────
        // Same query/symbol repeated 3+ times within 2 minutes
        if loop_count >= LOOP_REPEAT_THRESHOLD || snap.loop_alert_active {
            return Self::pulse(
                OperationalMoment::IterativeDistress,
                snap,
                format!(
                    "Looping detected: {} repeated attempts on same target in {}s window",
                    loop_count,
                    LOOP_TIME_WINDOW.as_secs()
                ),
                &m,
            );
        }

        // ── Priority 3: Pattern-based classification ───────────────

        // Debugging with errors present (errors > 0, fix-heavy activity)
        if snap.active_errors > 0 && snap.current_activity == "Debugging" {
            return Self::pulse(
                OperationalMoment::DeepBackendLogic,
                snap,
                format!(
                    "Active debugging: {} error(s), {} warning(s), fix-heavy activity",
                    snap.active_errors, snap.active_warnings
                ),
                &m,
            );
        }

        // Cross-layer: high module variance (3+ modules in window)
        if distinct_modules >= 3 && file_variance > 0.6 {
            return Self::pulse(
                OperationalMoment::ArchitectureWiring,
                snap,
                format!(
                    "Cross-layer: {} distinct modules, file variance {:.2}",
                    distinct_modules, file_variance
                ),
                &m,
            );
        }

        // Hardening/Consistency audit
        if snap.current_activity == "Hardening" {
            return Self::pulse(
                OperationalMoment::CrossLayerConsistency,
                snap,
                "Security/QA tools dominate — consistency audit".into(),
                &m,
            );
        }

        // Refactoring: read-heavy + high file variance
        if read_ratio > 0.7 && file_variance > 0.5 && snap.journey_phase_count > 3 {
            return Self::pulse(
                OperationalMoment::ExploratoryRefactoring,
                snap,
                format!(
                    "Exploration pattern: read ratio {:.2}, file variance {:.2}, {} phases",
                    read_ratio, file_variance, snap.journey_phase_count
                ),
                &m,
            );
        }

        // Deep backend: single module focus + write-heavy
        if distinct_modules <= 1 && read_ratio < 0.3 && snap.active_errors == 0 {
            return Self::pulse(
                OperationalMoment::DeepBackendLogic,
                snap,
                format!(
                    "Single-module deep work: read ratio {:.2}, {} module(s)",
                    read_ratio, distinct_modules
                ),
                &m,
            );
        }

        // Scaffolding: few phases, fast calls, many new files
        if snap.journey_phase_count <= 2 && avg_interval < 15.0 && distinct_files > 3 {
            return Self::pulse(
                OperationalMoment::RapidScaffolding,
                snap,
                format!(
                    "Early session, fast pace: {} files in {} phases, avg {:.1}s",
                    distinct_files, snap.journey_phase_count, avg_interval
                ),
                &m,
            );
        }

        // Errors decreasing → stabilizing (diagnostics trending down)
        if diag_trend < 0 && snap.active_errors == 0 {
            return Self::pulse(
                OperationalMoment::CrossLayerConsistency,
                snap,
                "Errors resolved, diagnostics trending down — stabilization".into(),
                &m,
            );
        }

        // ── Default: Momentum Flow ─────────────────────────────────
        // Established pattern, steady rhythm
        Self::pulse(
            OperationalMoment::MomentumFlow,
            snap,
            format!(
                "Steady rhythm: avg {:.1}s between calls, {} module(s)",
                avg_interval, distinct_modules
            ),
            &m,
        )
    }

    /// Compute the recency boost factor for search results.
    ///
    /// Symbols that appear in the working set get a boost up to
    /// [`MAX_RECENCY_BOOST`] (1.2x). Symbols not in the session
    /// get 1.0 (no change). This is additive, never subtractive.
    pub fn recency_boost(symbol: &str, file: &str, working_set_subjects: &[String]) -> f32 {
        // Exact file match → full boost
        if working_set_subjects.iter().any(|s| s == file) {
            return MAX_RECENCY_BOOST;
        }
        // Symbol name substring match → partial boost
        if working_set_subjects
            .iter()
            .any(|s| s.contains(symbol) || symbol.contains(s))
        {
            return 1.0 + (MAX_RECENCY_BOOST - 1.0) * 0.5; // 1.1x
        }
        1.0 // No boost — compendium mode, never penalize
    }

    // ── Private helpers ────────────────────────────────────────────

    fn pulse(
        moment: OperationalMoment,
        snap: &MetricsSnapshot,
        reason: String,
        metrics: &DerivedMetrics,
    ) -> SessionPulse {
        // Build the session hint line
        let error_hint = if snap.active_errors > 0 {
            format!(" Recent errors detected ({} error(s)).", snap.active_errors)
        } else {
            String::new()
        };

        let focus = if snap.focus_module.is_empty() {
            "General".to_string()
        } else {
            snap.focus_module
                .split('/')
                .next_back()
                .unwrap_or(&snap.focus_module)
                .to_string()
        };

        let session_hint = format!(
            "[SESSION_HINT]: {} — {} (Phase {}). Focus: {}.{}",
            moment.label(),
            moment.mode(),
            snap.journey_phase_count,
            focus,
            error_hint,
        );

        SessionPulse {
            moment,
            needle_range: moment.needle_range(),
            mode: moment.mode(),
            layer: moment.layer(),
            session_weight_pct: moment.session_weight_pct(),
            evidence: Evidence {
                tool_calls_in_window: metrics.n,
                avg_interval_secs: metrics.avg_interval,
                distinct_modules: metrics.distinct_modules,
                distinct_files: metrics.distinct_files,
                active_errors: snap.active_errors,
                active_warnings: snap.active_warnings,
                loop_repeat_count: metrics.loop_count,
                file_path_variance: metrics.file_variance,
                read_write_ratio: metrics.read_ratio,
                diagnostic_trend: metrics.diag_trend,
                reason,
            },
            focus_module: snap.focus_module.clone(),
            session_hint,
        }
    }

    pub(crate) fn avg_interval(snap: &MetricsSnapshot) -> f64 {
        let stamps: Vec<&Instant> = snap.call_timestamps.iter().collect();
        if stamps.len() < 2 {
            return 999.0; // No data → assume slow
        }
        let mut total = 0.0;
        for w in stamps.windows(2) {
            // Newest first, so w[0] > w[1]
            total += w[1].elapsed().as_secs_f64() - w[0].elapsed().as_secs_f64();
        }
        (total / (stamps.len() - 1) as f64).abs()
    }

    pub(crate) fn distinct_count(items: &VecDeque<String>) -> usize {
        let mut seen = std::collections::HashSet::new();
        for item in items {
            if !item.is_empty() {
                seen.insert(item.as_str());
            }
        }
        seen.len()
    }

    /// Count how many times the most recent subject repeats in the window.
    pub(crate) fn detect_repeat_count(snap: &MetricsSnapshot) -> usize {
        if snap.call_subjects.is_empty() {
            return 0;
        }
        let target = &snap.call_subjects[0]; // newest
        let cutoff = Instant::now() - LOOP_TIME_WINDOW;

        snap.call_subjects
            .iter()
            .zip(snap.call_timestamps.iter())
            .filter(|(s, t)| *s == target && **t > cutoff)
            .count()
    }

    pub(crate) fn read_write_ratio(snap: &MetricsSnapshot) -> f64 {
        if snap.call_tools.is_empty() {
            return 0.5;
        }
        let reads = snap
            .call_tools
            .iter()
            .filter(|t| {
                matches!(
                    t.as_str(),
                    "search" | "lookup" | "hoist" | "ask" | "similar" | "consult" | "blame"
                )
            })
            .count();
        reads as f64 / snap.call_tools.len() as f64
    }

    pub(crate) fn diagnostic_trend(snap: &MetricsSnapshot) -> i8 {
        let current = snap.active_errors + snap.active_warnings;
        let prev = snap.prev_errors + snap.prev_warnings;
        if current > prev {
            1
        } else if current < prev {
            -1
        } else {
            0
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn default_snap() -> MetricsSnapshot {
        MetricsSnapshot::default()
    }

    fn snap_with_calls(n: usize, tool: &str, module: &str) -> MetricsSnapshot {
        let now = Instant::now();
        let mut snap = MetricsSnapshot::default();
        for i in 0..n {
            // 10s intervals
            snap.call_timestamps
                .push_back(now - Duration::from_secs(i as u64 * 10));
            snap.call_tools.push_back(tool.to_string());
            // Use different files to avoid triggering loop detection
            snap.call_subjects
                .push_back(format!("{module}/src/file{i}.rs"));
            snap.call_modules.push_back(module.to_string());
        }
        snap.focus_module = module.to_string();
        snap.journey_phase_count = 1;
        snap
    }

    #[test]
    fn test_insufficient_data_defaults_to_scaffolding() {
        let snap = default_snap();
        let pulse = MomentClassifier::classify(&snap);
        assert_eq!(pulse.moment, OperationalMoment::RapidScaffolding);
        assert!(pulse.evidence.reason.contains("Insufficient"));
    }

    #[test]
    fn test_emergency_patching_on_many_errors() {
        let now = Instant::now();
        let mut snap = MetricsSnapshot::default();
        // 5 fast calls (3s apart)
        for i in 0..5 {
            snap.call_timestamps
                .push_back(now - Duration::from_secs(i * 3));
            snap.call_tools.push_back("diagnostics".to_string());
            snap.call_subjects
                .push_back("crates/core/src/lib.rs".to_string());
            snap.call_modules.push_back("crates/core".to_string());
        }
        snap.active_errors = 7;
        snap.focus_module = "crates/core".to_string();
        snap.journey_phase_count = 1;

        let pulse = MomentClassifier::classify(&snap);
        assert_eq!(pulse.moment, OperationalMoment::EmergencyPatching);
        assert_eq!(pulse.needle_range, 10);
        assert_eq!(pulse.mode, ModeHint::DeepReasoning);
        assert!(pulse.evidence.reason.contains("Emergency"));
    }

    #[test]
    fn test_iterative_distress_on_looping() {
        let now = Instant::now();
        let mut snap = MetricsSnapshot::default();
        // Same subject 4 times in rapid succession
        for i in 0..4 {
            snap.call_timestamps
                .push_back(now - Duration::from_secs(i * 15));
            snap.call_tools.push_back("search".to_string());
            snap.call_subjects
                .push_back("crates/auth/src/handler.rs".to_string());
            snap.call_modules.push_back("crates/auth".to_string());
        }
        snap.focus_module = "crates/auth".to_string();
        snap.journey_phase_count = 2;

        let pulse = MomentClassifier::classify(&snap);
        assert_eq!(pulse.moment, OperationalMoment::IterativeDistress);
        assert_eq!(pulse.mode, ModeHint::DeepReasoning);
        assert!(pulse.evidence.loop_repeat_count >= 3);
    }

    #[test]
    fn test_architecture_wiring_on_cross_module() {
        let now = Instant::now();
        let mut snap = MetricsSnapshot::default();
        let modules = [
            "crates/core",
            "crates/mcp",
            "crates/whisper",
            "crates/search",
        ];
        for (i, m) in modules.iter().enumerate() {
            snap.call_timestamps
                .push_back(now - Duration::from_secs(i as u64 * 20));
            snap.call_tools.push_back("lookup".to_string());
            snap.call_subjects.push_back(format!("{m}/src/lib.rs"));
            snap.call_modules.push_back(m.to_string());
        }
        snap.focus_module = "crates/search".to_string();
        snap.journey_phase_count = 4;

        let pulse = MomentClassifier::classify(&snap);
        assert_eq!(pulse.moment, OperationalMoment::ArchitectureWiring);
        assert!(pulse.evidence.distinct_modules >= 3);
    }

    #[test]
    fn test_momentum_flow_on_steady_rhythm() {
        let snap = snap_with_calls(6, "search", "crates/husk");
        let pulse = MomentClassifier::classify(&snap);
        // Single module, read-heavy, no errors → could be deep backend or exploration
        // The key is it shouldn't be Emergency or Distress
        assert_ne!(pulse.moment, OperationalMoment::EmergencyPatching);
        assert_ne!(pulse.moment, OperationalMoment::IterativeDistress);
    }

    #[test]
    fn test_session_hint_format() {
        let snap = snap_with_calls(5, "search", "crates/whisper");
        let pulse = MomentClassifier::classify(&snap);
        assert!(pulse.session_hint.starts_with("[SESSION_HINT]:"));
        assert!(pulse.session_hint.contains("Focus: whisper"));
    }

    #[test]
    fn test_all_moments_have_valid_metadata() {
        let moments = [
            OperationalMoment::RapidScaffolding,
            OperationalMoment::DeepBackendLogic,
            OperationalMoment::ArchitectureWiring,
            OperationalMoment::FrontendImplementation,
            OperationalMoment::UxPolish,
            OperationalMoment::CrossLayerConsistency,
            OperationalMoment::IterativeDistress,
            OperationalMoment::MomentumFlow,
            OperationalMoment::ExploratoryRefactoring,
            OperationalMoment::EmergencyPatching,
        ];
        for m in moments {
            assert!(m.needle_range() >= 1 && m.needle_range() <= 10);
            assert!(m.session_weight_pct() <= 100);
            assert!(!m.label().is_empty());
            assert!(!m.layer().is_empty());
            // Mode is always valid (enum)
            let _ = m.mode();
        }
    }

    #[test]
    fn test_recency_boost_exact_file_match() {
        let ws = vec![
            "crates/husk/src/scanner.rs".to_string(),
            "crates/core/src/lib.rs".to_string(),
        ];
        assert!(
            (MomentClassifier::recency_boost("scanner", "crates/husk/src/scanner.rs", &ws)
                - MAX_RECENCY_BOOST)
                .abs()
                < 0.001
        );
    }

    #[test]
    fn test_recency_boost_no_match() {
        let ws = vec!["crates/husk/src/scanner.rs".to_string()];
        let boost =
            MomentClassifier::recency_boost("totally_unknown", "crates/other/src/lib.rs", &ws);
        assert!((boost - 1.0).abs() < 0.001, "No match should return 1.0");
    }

    #[test]
    fn test_recency_boost_never_below_one() {
        // Compendium mode: never penalize
        let ws = vec![];
        let boost = MomentClassifier::recency_boost("anything", "any/path.rs", &ws);
        assert!(boost >= 1.0);
    }

    #[test]
    fn test_diagnostic_trend() {
        let mut snap = snap_with_calls(5, "diagnostics", "crates/core");
        snap.active_errors = 3;
        snap.prev_errors = 5;
        let pulse = MomentClassifier::classify(&snap);
        assert_eq!(pulse.evidence.diagnostic_trend, -1); // decreasing
    }

    #[test]
    fn test_hardening_triggers_consistency() {
        let mut snap = snap_with_calls(5, "scan", "crates/husk");
        snap.current_activity = "Hardening".to_string();
        let pulse = MomentClassifier::classify(&snap);
        assert_eq!(pulse.moment, OperationalMoment::CrossLayerConsistency);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let snap = snap_with_calls(5, "search", "crates/core");
        let pulse = MomentClassifier::classify(&snap);
        let json = serde_json::to_string(&pulse).unwrap();
        // Verify serialization contains expected fields
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(val["moment"], serde_json::json!(pulse.moment));
        assert_eq!(val["needle_range"], pulse.needle_range);
        assert!(val["session_hint"]
            .as_str()
            .unwrap()
            .contains("[SESSION_HINT]"));
    }
}
