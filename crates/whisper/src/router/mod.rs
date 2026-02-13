//! Intent Router — the Whisperer's brain.
//!
//! Classifies a natural-language query into an intent, extracts target
//! entities (files, symbols), then executes the appropriate subsystems
//! directly via Rust APIs (zero JSON-RPC overhead) and aggregates results.
//!
//! Level 0: Deterministic keyword heuristics.
//! Level 1 (future): Pluggable small-LLM classifier.
//!
//! # Module Layout (v4.0.0)
//!
//! - `intent.rs`          — keyword-based intent classification (EN/IT)
//! - `extraction.rs`      — 5-pass target extraction pipeline
//! - `context_builder.rs` — tier-aware smart context assembly
//! - `code.rs`            — code structure gathering
//! - `diagnostics.rs`     — compiler diagnostics gathering
//! - `history.rs`         — git history analysis
//! - `security.rs`        — security scanning & status

mod code;
mod context_builder;
mod diagnostics;
mod extraction;
mod history;
mod intent;
mod security;

use parking_lot::Mutex;
use serde::Serialize;
use tracing::{debug, info};

use synapseed_core::context::SynapseContext;
use synapseed_core::momentum::{ModelTier, MomentumEngine, SessionPhase};

use context_builder::SmartContextInput;
use extraction::is_vendor_path;

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

/// The full aggregated result from the Whisperer.
#[derive(Debug, Clone, Serialize)]
pub struct WhisperResult {
    pub intent: Intent,
    pub complexity: QueryComplexity,
    pub query: String,
    pub targets: Vec<Target>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<DiagnosticsContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<HistoryContext>,
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
    info!(query = query, raw = raw_injection, "Whisperer: Processing query");

    // ── Momentum: read tier + phase, check git staged (#52, #53, #54) ──
    let (tier, phase) = if let Some(engine) = ctx.get_extension::<Mutex<MomentumEngine>>() {
        let mut e = engine.lock();
        // Git-Context Alignment (#54): check for staged files
        let has_staged = context_builder::detect_git_staged(ctx);
        e.set_git_staged(has_staged);
        (e.tier(), e.phase())
    } else {
        (ModelTier::default(), SessionPhase::default())
    };
    debug!(tier = %tier, phase = %phase, "Whisperer: Momentum state");

    // ── Semantic Ballast (v3.7.0): Atomic tier forces raw injection ──
    let effective_raw = raw_injection || tier == ModelTier::Atomic;

    let mut intent_result = intent::classify_intent(query);
    let complexity = analyze_complexity(query);
    debug!(intent = ?intent_result, complexity = ?complexity, "Whisperer: Classified");

    let mut targets = extraction::extract_targets(query, ctx);

    // Atomic Greedy Pruning (v3.9.4): max 3 unique-file targets for sub-3B models.
    // Prefer diversity: one target per unique file path to maximize coverage.
    // Source-first ordering ensures implementation files come before test/vendor files.
    if tier == ModelTier::Atomic {
        // Drop vendor/static targets entirely — they waste precious Atomic slots
        let before = targets.len();
        targets.retain(|t| !t.file_path.as_deref().is_some_and(is_vendor_path));
        if targets.len() < before {
            debug!(before, after = targets.len(), "Whisper: dropped vendor/static targets");
        }
        if targets.len() > 3 {
            debug!(before = targets.len(), "Whisper: Atomic greedy pruning to 3 unique-file targets");
            let mut seen_files = std::collections::HashSet::new();
            targets.retain(|t| {
                let key = t.file_path.as_deref().unwrap_or("");
                seen_files.insert(key.to_string())
            });
            targets.truncate(3);
        }
    }
    debug!(target_count = targets.len(), "Whisperer: Extracted targets");

    // Intent Hardening: if query matched known symbols but intent is General,
    // promote to Explain — the user is asking about specific code entities.
    if intent_result == Intent::General
        && targets
            .iter()
            .any(|t| matches!(t.kind, TargetKind::Symbol))
    {
        debug!("Intent hardened: General -> Explain (query contains known symbols)");
        intent_result = Intent::Explain;
    }

    // Execute plan based on intent — each gather fn knows when to activate
    let diag = diagnostics::gather_diagnostics(&intent_result, &targets, ctx);
    let hist = history::gather_history(&intent_result, &targets, ctx);
    let code_ctx = code::gather_code_context(&intent_result, &targets, ctx);
    let sec_status = security::gather_security(&intent_result, &targets, ctx);

    // ── Raw Source Injection (v3.4.0) ──────────────────────────────
    // Atomic tier: always inject, with expanded budget (Semantic Ballast)
    let raw_sources = if effective_raw {
        context_builder::inject_raw_sources(&targets, ctx, tier == ModelTier::Atomic)
    } else {
        Vec::new()
    };

    let input = SmartContextInput {
        query,
        intent: &intent_result,
        complexity,
        diagnostics: &diag,
        history: &hist,
        code_context: &code_ctx,
        security_status: &sec_status,
        raw_injection: effective_raw,
        raw_sources: &raw_sources,
        tier,
        phase,
        project_root: ctx.project_root().display().to_string(),
    };

    let smart_context = context_builder::build_smart_context(input);

    // ── SID: Semantic Information Density ───────────────────────────
    // Formula: symbols_found / (prompt_tokens / 1000)
    // prompt_tokens ≈ smart_context.len() / 4 (rough char→token ratio)
    let symbols_found = code_ctx.as_ref().map_or(0, |c| c.symbols.len());
    let prompt_tokens = (smart_context.len() as f64 / 4.0).max(1.0);
    let sid = symbols_found as f64 / (prompt_tokens / 1000.0);

    WhisperResult {
        intent: intent_result,
        complexity,
        query: query.to_string(),
        targets,
        diagnostics: diag,
        history: hist,
        code_context: code_ctx,
        security_status: sec_status,
        smart_context,
        sid,
        raw_sources,
    }
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
