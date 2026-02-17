//! Intent Router — the Whisperer's brain.
//!
//! Classifies a natural-language query into an intent, extracts target
//! entities (files, symbols), then executes the appropriate subsystems
//! directly via Rust APIs (zero JSON-RPC overhead) and aggregates results.
//!
//! Level 0: Deterministic keyword heuristics.
//! Level 1 (future): Pluggable small-LLM classifier.
//!
//! # Module Layout (v5.0.0)
//!
//! - `intent.rs`          — keyword-based intent classification (EN/IT)
//! - `extraction.rs`      — 5-pass target extraction pipeline
//! - `coherence.rs`       — Coherence Gate: scatter detection & module clustering
//! - `context_builder.rs` — tier-aware smart context assembly
//! - `code.rs`            — code structure gathering
//! - `diagnostics.rs`     — compiler diagnostics gathering
//! - `history.rs`         — git history analysis
//! - `security.rs`        — security scanning & status
//! - `metrics.rs`         — per-stage pipeline timing (v5.0.0)

mod code;
mod coherence;
mod context_builder;
mod diagnostics;
mod extraction;
mod history;
mod intent;
pub mod metrics;
mod security;

use std::time::Instant;

use parking_lot::Mutex;
use serde::Serialize;
use tracing::{debug, info};

use synapseed_core::context::SynapseContext;
use synapseed_core::ledger::MomentClassifier;
use synapseed_core::momentum::{ModelTier, MomentumEngine, SessionPhase};
use synapseed_core::pulse::{PulseStore, COUNTER_FILE_TOUCHED, COUNTER_SYMBOL_TOUCHED};
use synapseed_core::recorder::FlightRecorder;

use context_builder::SmartContextInput;
use extraction::is_vendor_path;
use metrics::PipelineMetrics;

// ── Types ──────────────────────────────────────────────────────────────

/// Detected intent category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Intent {
    BugFix,
    Security,
    Explain,
    Refactor,
    General,
}

/// A target entity extracted from the query.
#[derive(Debug, Clone, Serialize)]
pub struct Target {
    pub kind: TargetKind,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_start: Option<usize>,
    /// Search relevance score propagated from the search index (v4.12.0).
    /// None for targets from non-search passes (explicit file refs, cortex fallback).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    File,
    Symbol,
}

/// Compiler diagnostics gathered for the query.
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticsContext {
    pub error_count: usize,
    pub warning_count: usize,
    pub items: Vec<serde_json::Value>,
}

/// Git history gathered for the query.
#[derive(Debug, Clone, Serialize)]
pub struct HistoryContext {
    pub file: String,
    pub total_commits: usize,
    pub hotspot_score: f64,
    pub risk: String,
    pub recent_commits: Vec<serde_json::Value>,
    pub top_authors: Vec<(String, usize)>,
    pub convergence_rate: f64,
    pub rigidity: f64,
    pub fix_chain_count: usize,
}

/// Code structure gathered for the query.
#[derive(Debug, Clone, Serialize)]
pub struct CodeContext {
    pub symbols: Vec<serde_json::Value>,
}

/// A raw source code snippet extracted from disk for a discovered symbol.
#[derive(Debug, Clone, Serialize)]
pub struct RawSource {
    pub file_path: String,
    pub line_start: usize,
    pub line_end: usize,
    pub source: String,
}

// ── Session State (v5.0.1: extracted from 15+ loose locals) ───────────

/// Accumulated session state built progressively across pipeline stages 1–5.
///
/// Groups momentum, intent, complexity, and target state that would
/// otherwise be scattered as loose local variables in `ask_with_options()`.
/// Passed by reference to sub-stages; mutated only for intent hardening
/// and target pruning.
pub(super) struct SessionState {
    /// Model tier from MomentumEngine (Stage 1).
    pub tier: ModelTier,
    /// Session phase from MomentumEngine (Stage 1).
    pub phase: SessionPhase,
    /// Whether raw source injection is active (explicit or ballast-triggered).
    pub effective_raw: bool,
    /// Classified intent — may be hardened from General→Explain (Stage 2/5).
    pub intent: Intent,
    /// Multi-intent scores for all non-zero intents (Stage 2).
    pub intent_scores: Vec<(String, usize)>,
    /// Query complexity: Quick/Standard/Deep (Stage 2).
    pub complexity: QueryComplexity,
    /// Extracted & pruned target entities — files, symbols (Stages 3–5).
    pub targets: Vec<Target>,
    /// Cognitive Ledger session hint (Stage 8).
    pub session_hint: Option<String>,
}

/// Context gathered from subsystems in Stages 6–7.
///
/// Separated from [`SessionState`] because these are produced by
/// independent sub-modules, not accumulated across stages.
pub(super) struct GatheredContext {
    pub diagnostics: Option<DiagnosticsContext>,
    pub histories: Vec<HistoryContext>,
    pub code_context: Option<CodeContext>,
    pub security_status: String,
    pub raw_sources: Vec<RawSource>,
}

/// The full aggregated result from the Whisperer.
#[derive(Debug, Clone, Serialize)]
pub struct WhisperResult {
    pub intent: Intent,
    /// All non-zero intent scores (v4.12.0: multi-intent awareness).
    /// Sorted by score descending. The `intent` field holds the winner.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub intent_scores: Vec<(String, usize)>,
    pub complexity: QueryComplexity,
    pub query: String,
    pub targets: Vec<Target>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<DiagnosticsContext>,
    /// Git history for all target files (v4.12.0: multi-file).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub histories: Vec<HistoryContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_context: Option<CodeContext>,
    pub security_status: String,
    pub smart_context: String,
    /// Semantic Information Density: symbols_found / (prompt_tokens / 1000).
    /// Higher = more useful signal per token budget.
    pub sid: f64,
    /// Raw source code snippets injected for discovered symbols.
    /// Exposed in JSON so external tools can consume code directly.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub raw_sources: Vec<RawSource>,
    /// Per-stage pipeline timing (v5.0.0).
    /// Every `ask` call captures microsecond-precision wall-clock times
    /// for each pipeline stage, enabling deterministic regression detection.
    pub pipeline_metrics: PipelineMetrics,
}

// ── Query Complexity (HCI Req 5: Mentor Mode) ─────────────────────────

/// How deep the Whisperer should go when building context.
/// Determined by simple string heuristics — no NLP, no external deps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryComplexity {
    /// Short/simple query → brief response, max 3 context sections
    Quick,
    /// Normal query → standard behavior
    Standard,
    /// Long/multi-part query → full context, cross-references
    Deep,
}

/// Classify query complexity from string heuristics.
/// Word count + question marks + code references → Quick/Standard/Deep.
pub fn analyze_complexity(query: &str) -> QueryComplexity {
    let word_count = query.split_whitespace().count();
    let question_marks = query.matches('?').count();
    let has_code_refs = query.contains("::")
        || query.contains("()")
        || query.contains(".rs")
        || query.contains(".py")
        || query.contains(".js");

    if word_count <= 4 && question_marks <= 1 && !has_code_refs {
        QueryComplexity::Quick
    } else if word_count >= 30 || question_marks > 1 || (word_count > 15 && has_code_refs) {
        QueryComplexity::Deep
    } else {
        QueryComplexity::Standard
    }
}

// ── Public API ─────────────────────────────────────────────────────────

/// Main entry point: analyze the query and return aggregated context.
///
/// Classifies intent, extracts targets, executes the right subsystems,
/// and returns everything the LLM needs in a single call.
///
/// HCI Req 5 (Mentor Mode): Response depth adapts to query complexity.
///
/// When `raw_injection` is true (v3.4.0+), the Whisperer reads the actual
/// source code for each discovered symbol and injects it verbatim into the
/// prompt, giving even sub-3B models enough context to answer accurately.
pub fn ask(query: &str, ctx: &SynapseContext) -> WhisperResult {
    ask_with_options(query, ctx, false)
}

/// Like [`ask`] but with explicit control over raw source injection.
pub fn ask_raw(query: &str, ctx: &SynapseContext, raw_injection: bool) -> WhisperResult {
    ask_with_options(query, ctx, raw_injection)
}

fn ask_with_options(query: &str, ctx: &SynapseContext, raw_injection: bool) -> WhisperResult {
    let pipeline_start = Instant::now();
    info!(query = query, raw = raw_injection, "Whisperer: Processing query");

    // ── Stage 1: Momentum — read tier + phase, check git staged ────
    let stage_start = Instant::now();
    let (tier, phase) = if let Some(engine) = ctx.get_extension::<Mutex<MomentumEngine>>() {
        let mut e = engine.lock();
        let has_staged = context_builder::detect_git_staged(ctx);
        e.set_git_staged(has_staged);
        (e.tier(), e.phase())
    } else {
        (ModelTier::default(), SessionPhase::default())
    };
    let momentum_us = stage_start.elapsed().as_micros() as u64;
    debug!(tier = %tier, phase = %phase, "Whisperer: Momentum state");

    // ── Semantic Ballast (v3.7.0): tiers that need it force raw injection ──
    let effective_raw = raw_injection || tier.needs_semantic_ballast();

    // ── Stage 2: Classify intent ───────────────────────────────────
    let stage_start = Instant::now();
    let intent = intent::classify_intent(query);
    let intent_scores = intent::classify_intent_scores(query);
    let complexity = analyze_complexity(query);
    let classify_us = stage_start.elapsed().as_micros() as u64;
    debug!(intent = ?intent, complexity = ?complexity, "Whisperer: Classified");

    // ── Stage 3: Extract targets ───────────────────────────────────
    let stage_start = Instant::now();
    let targets = extraction::extract_targets(query, ctx);
    let extract_us = stage_start.elapsed().as_micros() as u64;

    // ── Stage 3.5: Stale Target Filter (v5.1 — D10 fix) ───────────
    // Verify that extracted file paths still exist on disk.
    // During long refactoring sessions, files may be deleted/renamed
    // after indexing but before the next query.  Stale targets would
    // cause the LLM to reference non-existent files, triggering error
    // loops.  We silently discard them here.
    let root = ctx.project_root();
    let targets: Vec<_> = targets
        .into_iter()
        .filter(|t| {
            if let Some(ref fp) = t.file_path {
                let abs = root.join(fp);
                if !abs.exists() {
                    debug!(path = %fp, "Whisper: dropping stale target (file deleted/renamed)");
                    return false;
                }
            }
            true
        })
        .collect();
    let targets_before_prune = targets.len();

    // ── Build SessionState (v5.0.1) ────────────────────────────────
    let mut state = SessionState {
        tier,
        phase,
        effective_raw,
        intent,
        intent_scores,
        complexity,
        targets,
        session_hint: None,
    };

    // ── Stage 4: Greedy Pruning ────────────────────────────────────
    let stage_start = Instant::now();
    let max_files = tier.max_unique_files();
    if max_files < usize::MAX {
        let before = state.targets.len();
        state.targets.retain(|t| !t.file_path.as_deref().is_some_and(is_vendor_path));
        if state.targets.len() < before {
            debug!(before, after = state.targets.len(), "Whisper: dropped vendor/static targets");
        }
        if state.targets.len() > max_files {
            debug!(before = state.targets.len(), max_files, "Whisper: greedy pruning to max unique-file targets");
            let mut seen_files = std::collections::HashSet::new();
            state.targets.retain(|t| {
                let key = t.file_path.as_deref().unwrap_or("");
                seen_files.insert(key.to_string())
            });
            state.targets.truncate(max_files);
        }
    }
    let prune_us = stage_start.elapsed().as_micros() as u64;

    // ── Stage 5: Coherence Gate ────────────────────────────────────
    let stage_start = Instant::now();
    coherence::coherence_gate(&mut state.targets, tier);
    let coherence_us = stage_start.elapsed().as_micros() as u64;
    let targets_after_prune = state.targets.len();

    debug!(target_count = state.targets.len(), "Whisperer: Extracted targets");

    // Intent Hardening: if query matched known symbols but intent is General,
    // promote to Explain — the user is asking about specific code entities.
    if state.intent == Intent::General
        && state.targets
            .iter()
            .any(|t| matches!(t.kind, TargetKind::Symbol))
    {
        debug!("Intent hardened: General -> Explain (query contains known symbols)");
        state.intent = Intent::Explain;
    }

    // ── Stage 6: Gather context — D27 parallel dispatch ──────────────
    // Four independent subsystem calls run concurrently via std::thread::scope.
    // Each subsystem is thread-safe: DiagnosticStore uses RwLock, Historian uses
    // Mutex, CodeGraph uses DashMap, SecurityGuard is stateless.
    let stage_start = Instant::now();
    let (diagnostics, histories, code_context, security_status) = std::thread::scope(|s| {
        let intent = &state.intent;
        let targets = &state.targets;

        let d = s.spawn(|| diagnostics::gather_diagnostics(intent, targets, ctx));
        let h = s.spawn(|| history::gather_histories(intent, targets, ctx));
        let c = s.spawn(|| code::gather_code_context(intent, targets, ctx));
        let sec = s.spawn(|| security::gather_security(intent, targets, ctx));

        (
            d.join().unwrap_or(None),
            h.join().unwrap_or_default(),
            c.join().unwrap_or(None),
            sec.join().unwrap_or_else(|_| "GATHER_ERROR".to_string()),
        )
    });
    let gather_us = stage_start.elapsed().as_micros() as u64;

    // ── Stage 7: Raw Source Injection ──────────────────────────────
    let stage_start = Instant::now();
    let raw_sources = if state.effective_raw {
        context_builder::inject_raw_sources(&state.targets, ctx, tier)
    } else {
        Vec::new()
    };
    let raw_inject_us = stage_start.elapsed().as_micros() as u64;

    let gathered = GatheredContext {
        diagnostics,
        histories,
        code_context,
        security_status,
        raw_sources,
    };

    // ── Stage 8: Session — Pulse + Cognitive Ledger ────────────────
    let stage_start = Instant::now();
    if let Some(pulse) = ctx.get_extension::<PulseStore>() {
        for target in &state.targets {
            if let Some(ref path) = target.file_path {
                pulse.record(COUNTER_FILE_TOUCHED, path.as_str());
            }
            if matches!(target.kind, TargetKind::Symbol) {
                pulse.record(COUNTER_SYMBOL_TOUCHED, &target.name);
            }
        }
    }

    state.session_hint = ctx
        .get_extension::<Mutex<FlightRecorder>>()
        .map(|rec| {
            let recorder = rec.lock();
            let (errs, warns, prev_errs, prev_warns) = get_diagnostic_counts(ctx);
            let snap = recorder.build_metrics_snapshot(errs, warns, prev_errs, prev_warns);
            let pulse = MomentClassifier::classify(&snap);
            pulse.session_hint
        });
    let session_us = stage_start.elapsed().as_micros() as u64;

    // ── Stage 9: Build smart context ───────────────────────────────
    let stage_start = Instant::now();
    let input = SmartContextInput::from_session(
        query,
        &state,
        &gathered,
        ctx.project_root().display().to_string(),
    );
    let smart_context = context_builder::build_smart_context(input);
    let context_us = stage_start.elapsed().as_micros() as u64;

    // ── Stage 10: Finalize — SID + result assembly ─────────────────
    let stage_start = Instant::now();
    let symbols_found = gathered.code_context.as_ref().map_or(0, |c| c.symbols.len());
    let prompt_tokens = (smart_context.len() as f64 / 4.0).max(1.0);
    let sid = symbols_found as f64 / (prompt_tokens / 1000.0);
    let context_bytes = smart_context.len();
    let context_tokens = (context_bytes + 3) / 4;
    let finalize_us = stage_start.elapsed().as_micros() as u64;

    let total_us = pipeline_start.elapsed().as_micros() as u64;

    let pipeline_metrics = PipelineMetrics {
        momentum_us,
        classify_us,
        extract_us,
        prune_us,
        coherence_us,
        gather_us,
        raw_inject_us,
        session_us,
        context_us,
        finalize_us,
        total_us,
        targets_before_prune,
        targets_after_prune,
        context_bytes,
        context_tokens,
    };

    debug!(
        total_ms = total_us as f64 / 1000.0,
        bottleneck = pipeline_metrics.bottleneck(),
        "Whisperer: Pipeline complete"
    );

    // Record in aggregator if available
    if let Some(agg) = ctx.get_extension::<metrics::PipelineAggregator>() {
        agg.record(pipeline_metrics.clone());
    }

    WhisperResult {
        intent: state.intent,
        intent_scores: state.intent_scores,
        complexity: state.complexity,
        query: query.to_string(),
        targets: state.targets,
        diagnostics: gathered.diagnostics,
        histories: gathered.histories,
        code_context: gathered.code_context,
        security_status: gathered.security_status,
        smart_context,
        sid,
        raw_sources: gathered.raw_sources,
        pipeline_metrics,
    }
}

/// Extract diagnostic counts (current + previous) from the shadow compiler store.
fn get_diagnostic_counts(ctx: &SynapseContext) -> (u32, u32, u32, u32) {
    use synapseed_shadow_check::runner::DiagnosticStore;
    ctx.get_extension::<DiagnosticStore>()
        .map(|store| {
            let snap = store.snapshot();
            (
                snap.error_count as u32,
                snap.warning_count as u32,
                snap.prev_error_count as u32,
                snap.prev_warning_count as u32,
            )
        })
        .unwrap_or((0, 0, 0, 0))
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complexity_quick() {
        assert_eq!(analyze_complexity("what is this"), QueryComplexity::Quick);
        assert_eq!(analyze_complexity("help"), QueryComplexity::Quick);
        assert_eq!(analyze_complexity("fix it"), QueryComplexity::Quick);
    }

    #[test]
    fn test_complexity_standard() {
        assert_eq!(
            analyze_complexity("explain how the router works"),
            QueryComplexity::Standard
        );
        assert_eq!(
            analyze_complexity("what does the authentication module do"),
            QueryComplexity::Standard
        );
    }

    #[test]
    fn test_complexity_deep() {
        let long = "I need to understand how the authentication flow works across the entire codebase, including session management, token validation, and the security guard module. Can you also check for any vulnerabilities?";
        assert_eq!(analyze_complexity(long), QueryComplexity::Deep);
        // Multiple question marks
        assert_eq!(
            analyze_complexity("what is this? why does it fail? how to fix?"),
            QueryComplexity::Deep
        );
    }
}
