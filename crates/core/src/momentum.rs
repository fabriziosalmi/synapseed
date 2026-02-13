//! # Momentum Engine — Adaptive Brain
//!
//! Deterministic state machine for cognitive profiling and session phase detection.
//! Sub-ms arithmetic only, no LLM in the loop.
//!
//! ## Model Tiers
//! - **Atomic** (<3B params): flat markdown, max 2 targets, no JSON
//! - **Molecular** (7B–32B): balanced hybrid (default)
//! - **Galactic** (Cloud/SOTA): dense JSON, full context
//!
//! ## Session Phases
//! - **Discovery**: exploration-heavy (hoist, search, lookup dominate)
//! - **Implementation**: write-heavy (diagnostics, check, quickfix dominate)
//! - **Stabilization**: hardening (scan, diagnose, git-staged dominate)

use serde::{Deserialize, Serialize};

// ── Model Tier ─────────────────────────────────────────────────────────

/// Cognitive tier reflecting the downstream model's capacity.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelTier {
    /// Sub-3B parameter models (ollama tiny, phi-2, tinyllama).
    /// Output: flat markdown, max 2 symbol targets, no JSON blocks.
    Atomic,
    /// 7B–32B models (mistral, codellama, deepseek-coder).
    /// Output: balanced hybrid — human summary + structured sections.
    #[default]
    Molecular,
    /// Cloud/SOTA models (Claude, GPT-4, Gemini Pro).
    /// Output: dense JSON context, full symbol injection, all sections.
    Galactic,
}

impl ModelTier {
    /// Max symbol targets to include in context for this tier.
    pub fn max_targets(&self) -> usize {
        match self {
            ModelTier::Atomic => 2,
            ModelTier::Molecular => 5,
            ModelTier::Galactic => 10,
        }
    }

    /// Whether to include raw JSON blocks in the smart context.
    pub fn allows_json(&self) -> bool {
        !matches!(self, ModelTier::Atomic)
    }

    /// Detect tier from MCP client name (case-insensitive).
    pub fn from_client_name(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.contains("ollama")
            || lower.contains("lm-studio")
            || lower.contains("lmstudio")
            || lower.contains("llamafile")
            || lower.contains("localai")
            || lower.contains("kobold")
        {
            ModelTier::Atomic
        } else if lower.contains("claude")
            || lower.contains("anthropic")
            || lower.contains("gpt")
            || lower.contains("openai")
            || lower.contains("gemini")
            || lower.contains("cursor")
            || lower.contains("windsurf")
            || lower.contains("zed")
        {
            ModelTier::Galactic
        } else {
            ModelTier::Molecular
        }
    }

    /// Parse from DNA config string.
    pub fn from_config(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "atomic" => Some(ModelTier::Atomic),
            "molecular" => Some(ModelTier::Molecular),
            "galactic" => Some(ModelTier::Galactic),
            _ => None,
        }
    }
}


impl std::fmt::Display for ModelTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelTier::Atomic => write!(f, "Atomic"),
            ModelTier::Molecular => write!(f, "Molecular"),
            ModelTier::Galactic => write!(f, "Galactic"),
        }
    }
}

// ── Session Phase ──────────────────────────────────────────────────────

/// Current session phase, determined by tool invocation patterns.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPhase {
    /// Exploration: hoist, search, lookup, ask dominate.
    #[default]
    Discovery,
    /// Active coding: diagnostics, check_command, quickfix dominate.
    Implementation,
    /// Hardening: scan_security, diagnose, architect, git-staged dominate.
    Stabilization,
}

impl std::fmt::Display for SessionPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionPhase::Discovery => write!(f, "Discovery"),
            SessionPhase::Implementation => write!(f, "Implementation"),
            SessionPhase::Stabilization => write!(f, "Stabilization"),
        }
    }
}

// ── Tool Categories ────────────────────────────────────────────────────

/// Categorize a tool name into a phase signal.
fn tool_phase_signal(tool: &str) -> Option<SessionPhase> {
    match tool {
        // Discovery tools
        "hoist" | "search" | "lookup" | "ask" | "similar" | "consult" => {
            Some(SessionPhase::Discovery)
        }
        // Implementation tools
        "diagnostics" | "check" | "quickfix" | "blame" | "intent" => {
            Some(SessionPhase::Implementation)
        }
        // Stabilization tools
        "scan" | "diagnose" | "architect" | "janitor" | "janitor-fix" | "oracle"
        | "verify_path" => Some(SessionPhase::Stabilization),
        // Neutral tools (don't shift phase)
        _ => None,
    }
}

// ── Momentum Engine ────────────────────────────────────────────────────

/// Sliding window size for tool invocation tracking.
const WINDOW_SIZE: usize = 10;

/// Deterministic state machine tracking session momentum.
///
/// Pure arithmetic — no I/O, no async, sub-microsecond per call.
/// Designed to be stored in `SynapseContext` via the extension registry.
#[derive(Debug, Clone)]
pub struct MomentumEngine {
    /// Circular buffer of recent tool phase signals.
    window: Vec<SessionPhase>,
    /// Current detected phase.
    current_phase: SessionPhase,
    /// Whether git has staged files (forces Stabilization).
    git_staged: bool,
    /// Detected model tier (from client fingerprinting or DNA).
    model_tier: ModelTier,
}

impl MomentumEngine {
    /// Create a new engine with the given model tier.
    pub fn new(tier: ModelTier) -> Self {
        Self {
            window: Vec::with_capacity(WINDOW_SIZE),
            current_phase: SessionPhase::Discovery,
            git_staged: false,
            model_tier: tier,
        }
    }

    /// Record a tool invocation. Recalculates the phase.
    pub fn record_tool(&mut self, tool_name: &str) {
        if let Some(signal) = tool_phase_signal(tool_name) {
            if self.window.len() >= WINDOW_SIZE {
                self.window.remove(0);
            }
            self.window.push(signal);
            self.recalculate();
        }
    }

    /// Set whether git has staged files (forces Stabilization).
    pub fn set_git_staged(&mut self, staged: bool) {
        self.git_staged = staged;
        if staged {
            self.current_phase = SessionPhase::Stabilization;
        } else {
            // When unstaged, recalculate from the sliding window
            self.recalculate();
        }
    }

    /// Get the current session phase.
    pub fn phase(&self) -> SessionPhase {
        if self.git_staged {
            return SessionPhase::Stabilization;
        }
        self.current_phase
    }

    /// Get the configured model tier.
    pub fn tier(&self) -> ModelTier {
        self.model_tier
    }

    /// Set model tier (e.g., after DNA override).
    pub fn set_tier(&mut self, tier: ModelTier) {
        self.model_tier = tier;
    }

    /// Recalculate phase from the sliding window.
    fn recalculate(&mut self) {
        if self.git_staged {
            self.current_phase = SessionPhase::Stabilization;
            return;
        }
        if self.window.is_empty() {
            return;
        }

        let mut discovery = 0u32;
        let mut implementation = 0u32;
        let mut stabilization = 0u32;

        for signal in &self.window {
            match signal {
                SessionPhase::Discovery => discovery += 1,
                SessionPhase::Implementation => implementation += 1,
                SessionPhase::Stabilization => stabilization += 1,
            }
        }

        let max = discovery.max(implementation).max(stabilization);
        // On tie, priority: Stabilization > Implementation > Discovery
        self.current_phase = if stabilization == max {
            SessionPhase::Stabilization
        } else if implementation == max {
            SessionPhase::Implementation
        } else {
            SessionPhase::Discovery
        };
    }

    /// Get a concise summary for inclusion in smart context.
    pub fn summary(&self) -> String {
        format!(
            "SESSION: Phase={}, Tier={}, Window={}/{}",
            self.phase(),
            self.model_tier,
            self.window.len(),
            WINDOW_SIZE
        )
    }
}

impl Default for MomentumEngine {
    fn default() -> Self {
        Self::new(ModelTier::default())
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_tier_defaults_to_molecular() {
        assert_eq!(ModelTier::default(), ModelTier::Molecular);
    }

    #[test]
    fn test_model_tier_from_client_name() {
        assert_eq!(ModelTier::from_client_name("ollama"), ModelTier::Atomic);
        assert_eq!(ModelTier::from_client_name("LM-Studio"), ModelTier::Atomic);
        assert_eq!(
            ModelTier::from_client_name("Claude Desktop"),
            ModelTier::Galactic
        );
        assert_eq!(ModelTier::from_client_name("cursor"), ModelTier::Galactic);
        assert_eq!(
            ModelTier::from_client_name("my-custom-app"),
            ModelTier::Molecular
        );
    }

    #[test]
    fn test_model_tier_from_config() {
        assert_eq!(ModelTier::from_config("atomic"), Some(ModelTier::Atomic));
        assert_eq!(
            ModelTier::from_config("GALACTIC"),
            Some(ModelTier::Galactic)
        );
        assert_eq!(ModelTier::from_config("invalid"), None);
    }

    #[test]
    fn test_model_tier_max_targets() {
        assert_eq!(ModelTier::Atomic.max_targets(), 2);
        assert_eq!(ModelTier::Molecular.max_targets(), 5);
        assert_eq!(ModelTier::Galactic.max_targets(), 10);
    }

    #[test]
    fn test_model_tier_json_allowed() {
        assert!(!ModelTier::Atomic.allows_json());
        assert!(ModelTier::Molecular.allows_json());
        assert!(ModelTier::Galactic.allows_json());
    }

    #[test]
    fn test_phase_starts_discovery() {
        let engine = MomentumEngine::default();
        assert_eq!(engine.phase(), SessionPhase::Discovery);
    }

    #[test]
    fn test_phase_transitions_to_implementation() {
        let mut engine = MomentumEngine::default();
        for _ in 0..5 {
            engine.record_tool("diagnostics");
        }
        assert_eq!(engine.phase(), SessionPhase::Implementation);
    }

    #[test]
    fn test_phase_transitions_to_stabilization() {
        let mut engine = MomentumEngine::default();
        for _ in 0..5 {
            engine.record_tool("scan");
        }
        assert_eq!(engine.phase(), SessionPhase::Stabilization);
    }

    #[test]
    fn test_git_staged_forces_stabilization() {
        let mut engine = MomentumEngine::default();
        // Fill with Discovery signals
        for _ in 0..5 {
            engine.record_tool("search");
        }
        assert_eq!(engine.phase(), SessionPhase::Discovery);

        // Staged files override
        engine.set_git_staged(true);
        assert_eq!(engine.phase(), SessionPhase::Stabilization);

        // Unstage reverts
        engine.set_git_staged(false);
        assert_eq!(engine.phase(), SessionPhase::Discovery);
    }

    #[test]
    fn test_sliding_window_eviction() {
        let mut engine = MomentumEngine::default();
        // Fill window with Discovery
        for _ in 0..10 {
            engine.record_tool("search");
        }
        assert_eq!(engine.phase(), SessionPhase::Discovery);

        // Overwrite with Implementation
        for _ in 0..10 {
            engine.record_tool("diagnostics");
        }
        assert_eq!(engine.phase(), SessionPhase::Implementation);
    }

    #[test]
    fn test_neutral_tools_dont_shift_phase() {
        let mut engine = MomentumEngine::default();
        engine.record_tool("hoist");
        assert_eq!(engine.phase(), SessionPhase::Discovery);

        // "train" and "reset-telemetry" are neutral
        engine.record_tool("train");
        engine.record_tool("reset-telemetry");
        assert_eq!(engine.phase(), SessionPhase::Discovery);
    }

    #[test]
    fn test_tie_breaks_priority() {
        let mut engine = MomentumEngine::default();
        // 1 Discovery + 1 Stabilization → Stabilization wins on tie
        engine.record_tool("search");
        engine.record_tool("scan");
        assert_eq!(engine.phase(), SessionPhase::Stabilization);
    }

    #[test]
    fn test_summary_format() {
        let engine = MomentumEngine::new(ModelTier::Galactic);
        let s = engine.summary();
        assert!(s.contains("Phase=Discovery"));
        assert!(s.contains("Tier=Galactic"));
    }

    #[test]
    fn test_tool_phase_signal() {
        assert_eq!(tool_phase_signal("hoist"), Some(SessionPhase::Discovery));
        assert_eq!(
            tool_phase_signal("diagnostics"),
            Some(SessionPhase::Implementation)
        );
        assert_eq!(
            tool_phase_signal("scan"),
            Some(SessionPhase::Stabilization)
        );
        assert_eq!(tool_phase_signal("train"), None);
    }
}
