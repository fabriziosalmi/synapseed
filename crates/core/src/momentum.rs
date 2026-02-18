//! # Momentum Engine — Adaptive Brain
//!
//! Deterministic state machine for cognitive profiling and session phase detection.
//! Sub-ms arithmetic only, no LLM in the loop.
//!
//! ## Model Tiers (Context Budgeting Strategy v5.0)
//!
//! Token budgets calibrated to model attention capacity. The 80% Rule ensures
//! the ContextBuilder targets 80% of the tier budget, reserving 20% for the
//! system prompt and user query.
//!
//! | Tier       | Model Size   | Token Budget | Search Strategy  |
//! |------------|-------------|-------------|------------------|
//! | **Atomic**     | < 1B         | 2K           | Chirurgica       |
//! | **Molecular**  | 1B – 4B      | 8K           | Relazionale      |
//! | **Galactic**   | 4B – 14B     | 16K          | Architetturale   |
//! | **Universal**  | > 14B / Cloud| 32K+         | Olistica         |
//!
//! ## Session Phases
//! - **Discovery**: exploration-heavy (hoist, search, lookup dominate)
//! - **Implementation**: write-heavy (diagnostics, check, quickfix dominate)
//! - **Stabilization**: hardening (scan, diagnose, git-staged dominate)

use serde::{Deserialize, Serialize};

// ── Model Tier ─────────────────────────────────────────────────────────

/// Cognitive tier reflecting the downstream model's capacity.
///
/// **Context Budgeting Strategy v5.0** — all budgets derive from a single
/// `token_budget()` parameter.  The 80% Rule reserves 20% headroom for the
/// system prompt and user query.  Sub-budgets (source injection, critical
/// symbols) are computed as proportions so the whole system re-calibrates
/// when you change the tier.
///
/// ```text
/// token_budget ──► effective_tokens (×0.80)
///                       │
///                       ├─► overhead (preamble, diagnostics, summaries)
///                       │
///                       └─► source_tokens = effective − overhead
///                               │
///                               ├─► source_char_budget (×CHARS_PER_TOKEN)
///                               │
///                               └─► critical_char_budget (proportional slice)
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelTier {
    /// < 1B parameter models (qwen 0.6B, phi-2, tinyllama).
    /// Strategy: *Chirurgica* — only the exact symbol, zero boilerplate.
    /// Token budget: 2K.  Few attention heads → "pixelation" above 2K.
    Atomic,
    /// 1B–4B models (qwen 1.7B, phi-3-mini, gemma-2B).
    /// Strategy: *Relazionale* — can follow links between 2-3 files.
    /// Token budget: 8K.  Supports definitions + test snippets.
    #[default]
    Molecular,
    /// 4B–14B models (llama 8B, mistral, codellama, deepseek-coder).
    /// Strategy: *Architetturale* — understands patterns across modules.
    /// Token budget: 16K.  Entire modules + dependency graph.
    Galactic,
    /// > 14B / Cloud models (Claude, GPT-4, Gemini Pro).
    /// > Strategy: *Olistica* — massive synthesis capacity.
    /// > Token budget: 32K+.  Full module history + cross-references.
    Universal,
}

/// Approximate chars-per-token ratio for budget conversion.
/// Rust/Python average ~3.5–4.5; we use 4 as a safe middle ground.
const CHARS_PER_TOKEN: usize = 4;

/// The 80% Rule: target 80% of the tier budget to leave headroom for
/// the system prompt, user query, and response scaffolding.
const BUDGET_FILL_RATIO: f64 = 0.80;

// ── Tier Lookup Table (v5.0.1: table-driven, #71) ─────────────────────

/// Per-tier scalar parameters.  Indexed by `ModelTier::idx()`.
///
/// Columns: (token_budget, overhead_tokens, critical_ratio×100,
///           min_source_lines, max_targets, max_clusters, max_unique_files)
///
/// Using integer ratio (×100) to keep the table `const`-friendly;
/// `critical_char_budget()` divides by 100 at runtime.
const TIER_TABLE: [(usize, usize, usize, usize, usize, usize, usize); 4] = [
    // Atomic:    2K budget, 250 overhead, 35% critical, 15 min lines, 2 targets, 1 cluster, 2 files
    (2_048, 250, 35, 15, 2, 1, 2),
    // Molecular: 8K budget, 500 overhead, 30% critical, 10 min lines, 5 targets, 2 clusters, 4 files
    (8_192, 500, 30, 10, 5, 2, 4),
    // Galactic:  16K budget, 800 overhead, 25% critical, 0 min lines, 10 targets, 3 clusters, 8 files
    (16_384, 800, 25, 0, 10, 3, 8),
    // Universal: 32K budget, 1200 overhead, 20% critical, 0 min lines, 15 targets, 5 clusters, MAX files
    (32_768, 1_200, 20, 0, 15, 5, usize::MAX),
];

/// Tool → phase signal mapping (v5.0.1: table-driven, #71).
const TOOL_PHASE_TABLE: &[(&str, SessionPhase)] = &[
    // Discovery tools
    ("hoist", SessionPhase::Discovery),
    ("search", SessionPhase::Discovery),
    ("lookup", SessionPhase::Discovery),
    ("ask", SessionPhase::Discovery),
    ("similar", SessionPhase::Discovery),
    ("consult", SessionPhase::Discovery),
    // Implementation tools
    ("diagnostics", SessionPhase::Implementation),
    ("check", SessionPhase::Implementation),
    ("quickfix", SessionPhase::Implementation),
    ("blame", SessionPhase::Implementation),
    ("intent", SessionPhase::Implementation),
    // Stabilization tools
    ("scan", SessionPhase::Stabilization),
    ("diagnose", SessionPhase::Stabilization),
    ("architect", SessionPhase::Stabilization),
    ("janitor", SessionPhase::Stabilization),
    ("janitor-fix", SessionPhase::Stabilization),
    ("oracle", SessionPhase::Stabilization),
    ("verify_path", SessionPhase::Stabilization),
];

/// Client name → tier detection rules (v5.0.1: table-driven, #71).
const CLIENT_TIER_TABLE: &[(&[&str], ModelTier)] = &[
    (
        &[
            "ollama",
            "lm-studio",
            "lmstudio",
            "llamafile",
            "localai",
            "kobold",
        ],
        ModelTier::Atomic,
    ),
    (
        &["claude", "anthropic", "gpt", "openai", "gemini"],
        ModelTier::Universal,
    ),
    (
        &["cursor", "windsurf", "zed", "continue"],
        ModelTier::Galactic,
    ),
];

impl ModelTier {
    // ═══════════════════════════════════════════════════════════════
    // Table index — maps enum variant to TIER_TABLE row
    // ═══════════════════════════════════════════════════════════════

    const fn idx(&self) -> usize {
        match self {
            ModelTier::Atomic => 0,
            ModelTier::Molecular => 1,
            ModelTier::Galactic => 2,
            ModelTier::Universal => 3,
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Context Budgeting (v5.0) — everything derives from token_budget
    // ═══════════════════════════════════════════════════════════════

    /// Raw token budget for this tier (the "master knob").
    /// Every other budget is derived from this value.
    pub fn token_budget(&self) -> usize {
        TIER_TABLE[self.idx()].0
    }

    /// Tokens reserved for non-source sections of the context:
    /// preamble, ENVIRONMENT header, diagnostics, symbol summary, section
    /// headers.  Scales with tier complexity (Atomic is flat text, Universal
    /// has rich cross-referenced sections).
    fn overhead_tokens(&self) -> usize {
        TIER_TABLE[self.idx()].1
    }

    /// Tokens available for source code injection after overhead and the 80% rule.
    fn source_tokens(&self) -> usize {
        let effective = (self.token_budget() as f64 * BUDGET_FILL_RATIO) as usize;
        effective.saturating_sub(self.overhead_tokens())
    }

    /// Char budget for raw source injection (the shared pool).
    /// Derived: `source_tokens × CHARS_PER_TOKEN`.
    pub fn source_char_budget(&self) -> usize {
        self.source_tokens() * CHARS_PER_TOKEN
    }

    /// Dedicated char budget for critical symbols (DNA-configured).
    /// A proportional slice of the source pool — smaller models devote a
    /// larger %, so the few symbols they see are guaranteed to be complete.
    pub fn critical_char_budget(&self) -> usize {
        let ratio = TIER_TABLE[self.idx()].2 as f64 / 100.0;
        (self.source_char_budget() as f64 * ratio) as usize
    }

    /// Minimum lines per injected snippet.  Small models need more grounding
    /// context per-symbol to reduce hallucination, but get fewer symbols.
    pub fn min_source_lines(&self) -> usize {
        TIER_TABLE[self.idx()].3
    }

    // ═══════════════════════════════════════════════════════════════
    // Target & Cluster Limits
    // ═══════════════════════════════════════════════════════════════

    /// Max symbol targets to include in context for this tier.
    pub fn max_targets(&self) -> usize {
        TIER_TABLE[self.idx()].4
    }

    /// Max module clusters the coherence gate keeps.
    pub fn max_clusters(&self) -> usize {
        TIER_TABLE[self.idx()].5
    }

    /// Max unique-file targets for greedy pruning in the router.
    pub fn max_unique_files(&self) -> usize {
        TIER_TABLE[self.idx()].6
    }

    // ═══════════════════════════════════════════════════════════════
    // Feature Gates
    // ═══════════════════════════════════════════════════════════════

    /// Whether to include raw JSON blocks in the smart context.
    pub fn allows_json(&self) -> bool {
        !matches!(self, ModelTier::Atomic)
    }

    /// Whether this tier needs Semantic Ballast (forced raw injection +
    /// `@@@ START_OF_TRUTH` delimiters + language reinforcement).
    pub fn needs_semantic_ballast(&self) -> bool {
        matches!(self, ModelTier::Atomic)
    }

    // ═══════════════════════════════════════════════════════════════
    // Detection
    // ═══════════════════════════════════════════════════════════════

    /// Detect tier from MCP client name (case-insensitive, table-driven).
    pub fn from_client_name(name: &str) -> Self {
        let lower = name.to_lowercase();
        for &(patterns, tier) in CLIENT_TIER_TABLE {
            if patterns.iter().any(|p| lower.contains(p)) {
                return tier;
            }
        }
        ModelTier::Molecular
    }

    /// Parse from DNA config string.
    pub fn from_config(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "atomic" => Some(ModelTier::Atomic),
            "molecular" => Some(ModelTier::Molecular),
            "galactic" => Some(ModelTier::Galactic),
            "universal" => Some(ModelTier::Universal),
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
            ModelTier::Universal => write!(f, "Universal"),
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

/// Categorize a tool name into a phase signal (table-driven, v5.0.1).
fn tool_phase_signal(tool: &str) -> Option<SessionPhase> {
    TOOL_PHASE_TABLE
        .iter()
        .find(|&&(name, _)| name == tool)
        .map(|&(_, phase)| phase)
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
            ModelTier::Universal
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
        assert_eq!(
            ModelTier::from_config("universal"),
            Some(ModelTier::Universal)
        );
        assert_eq!(ModelTier::from_config("invalid"), None);
    }

    #[test]
    fn test_model_tier_max_targets() {
        assert_eq!(ModelTier::Atomic.max_targets(), 2);
        assert_eq!(ModelTier::Molecular.max_targets(), 5);
        assert_eq!(ModelTier::Galactic.max_targets(), 10);
        assert_eq!(ModelTier::Universal.max_targets(), 15);
    }

    #[test]
    fn test_model_tier_json_allowed() {
        assert!(!ModelTier::Atomic.allows_json());
        assert!(ModelTier::Molecular.allows_json());
        assert!(ModelTier::Galactic.allows_json());
        assert!(ModelTier::Universal.allows_json());
    }

    // ── Context Budgeting v5.0 Tests ──────────────────────────────

    #[test]
    fn test_token_budgets_ascending() {
        let tiers = [
            ModelTier::Atomic,
            ModelTier::Molecular,
            ModelTier::Galactic,
            ModelTier::Universal,
        ];
        for w in tiers.windows(2) {
            assert!(
                w[0].token_budget() < w[1].token_budget(),
                "{:?} budget should be < {:?} budget",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn test_source_char_budget_derived_from_token_budget() {
        // source_char_budget must be strictly less than token_budget * 4
        // (because of the 80% rule and overhead deduction)
        for tier in [
            ModelTier::Atomic,
            ModelTier::Molecular,
            ModelTier::Galactic,
            ModelTier::Universal,
        ] {
            let max_chars = tier.token_budget() * 4;
            assert!(
                tier.source_char_budget() < max_chars,
                "{:?}: source_char_budget {} should be < {}",
                tier,
                tier.source_char_budget(),
                max_chars
            );
            // But it should still be a significant portion (> 50%)
            assert!(
                tier.source_char_budget() > max_chars / 2,
                "{:?}: source_char_budget {} too small vs max {}",
                tier,
                tier.source_char_budget(),
                max_chars
            );
        }
    }

    #[test]
    fn test_critical_budget_is_subset_of_source_budget() {
        for tier in [
            ModelTier::Atomic,
            ModelTier::Molecular,
            ModelTier::Galactic,
            ModelTier::Universal,
        ] {
            assert!(
                tier.critical_char_budget() < tier.source_char_budget(),
                "{:?}: critical {} should be < source {}",
                tier,
                tier.critical_char_budget(),
                tier.source_char_budget()
            );
        }
    }

    #[test]
    fn test_atomic_budget_sanity() {
        // Atomic: 2K tokens → source should be roughly 5-7K chars
        let src = ModelTier::Atomic.source_char_budget();
        assert!(
            (4_000..=8_000).contains(&src),
            "Atomic source_char_budget = {src}"
        );
        let crit = ModelTier::Atomic.critical_char_budget();
        assert!(
            (1_000..=3_000).contains(&crit),
            "Atomic critical_char_budget = {crit}"
        );
    }

    #[test]
    fn test_molecular_budget_sanity() {
        // Molecular: 8K tokens → source should be roughly 20-28K chars
        let src = ModelTier::Molecular.source_char_budget();
        assert!(
            (20_000..=30_000).contains(&src),
            "Molecular source_char_budget = {src}"
        );
    }

    #[test]
    fn test_needs_semantic_ballast() {
        assert!(ModelTier::Atomic.needs_semantic_ballast());
        assert!(!ModelTier::Molecular.needs_semantic_ballast());
        assert!(!ModelTier::Galactic.needs_semantic_ballast());
        assert!(!ModelTier::Universal.needs_semantic_ballast());
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
        assert_eq!(tool_phase_signal("scan"), Some(SessionPhase::Stabilization));
        assert_eq!(tool_phase_signal("train"), None);
    }
}
