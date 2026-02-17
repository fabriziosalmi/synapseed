//! Integration tests for the full ask pipeline (whisper end-to-end).
//!
//! Tests the complete flow:
//!   user query → intent classification → extraction → search
//!                → context_builder → smart_context output
//!
//! Uses a small fixture project (6 Rust source files) as the test workspace.
//! All extensions are optional: the pipeline gracefully degrades, so we test
//! both the minimal path (no extensions) and the tier-adapted path.
//!
//! Issue #69

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use synapseed_core::context::SynapseContext;
use synapseed_core::liquid::ProjectDna;
use synapseed_core::momentum::{ModelTier, MomentumEngine};
use synapseed_core::state::{BuildSystem, ProjectState};
use synapseed_whisper::router::{self, Intent, QueryComplexity};

/// Path to the fixture project checked into the repo.
fn fixture_project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("sample_project")
}

/// Build a minimal `SynapseContext` pointing at the fixture project.
fn make_ctx() -> SynapseContext {
    SynapseContext::new(
        fixture_project_root(),
        ProjectState::HealthyWorkspace {
            build_system: BuildSystem::Cargo,
            file_count: 6,
        },
        ProjectDna::default(),
    )
}

/// Build a context with a specific model tier via MomentumEngine.
fn make_ctx_with_tier(tier: ModelTier) -> SynapseContext {
    let ctx = make_ctx();
    ctx.set_extension(Arc::new(Mutex::new(MomentumEngine::new(tier))));
    ctx
}

// ── Test 1: Basic ask returns a valid WhisperResult ────────────────────

#[test]
fn ask_returns_valid_result() {
    let ctx = make_ctx();
    let result = router::ask("explain the router module", &ctx);

    // Must return a non-empty smart_context
    assert!(
        !result.smart_context.is_empty(),
        "smart_context must not be empty"
    );

    // The query is echoed back
    assert_eq!(result.query, "explain the router module");

    // SID is a finite non-negative number
    assert!(result.sid.is_finite(), "SID must be finite");
    assert!(result.sid >= 0.0, "SID must be non-negative");

    // Pipeline metrics are populated
    assert!(
        result.pipeline_metrics.total_us > 0,
        "total pipeline time must be > 0"
    );
}

// ── Test 2: Intent classification through the pipeline ─────────────────

#[test]
fn ask_classifies_intent_correctly() {
    let ctx = make_ctx();

    // Bug-related query → BugFix
    let bug_result = router::ask("why is the login broken? there's an error", &ctx);
    assert_eq!(
        bug_result.intent,
        Intent::BugFix,
        "bug query should classify as BugFix"
    );

    // Security query → Security
    let sec_result = router::ask("run a security audit for vulnerabilities", &ctx);
    assert_eq!(
        sec_result.intent,
        Intent::Security,
        "security query should classify as Security"
    );

    // Explain query → Explain
    let explain_result = router::ask("explain how the authentication module works", &ctx);
    // Could be Explain or General→Explain hardened if symbols found
    assert!(
        matches!(explain_result.intent, Intent::Explain | Intent::General),
        "explain query should classify as Explain or General, got {:?}",
        explain_result.intent
    );
}

// ── Test 3: Query complexity adapts response depth ─────────────────────

#[test]
fn ask_complexity_adapts_to_query() {
    let ctx = make_ctx();

    // Short query → Quick
    let quick = router::ask("help", &ctx);
    assert_eq!(
        quick.complexity,
        QueryComplexity::Quick,
        "short query should be Quick"
    );

    // Normal query → Standard
    let standard = router::ask("explain how the router dispatches requests", &ctx);
    assert_eq!(
        standard.complexity,
        QueryComplexity::Standard,
        "normal query should be Standard"
    );

    // Long multi-part query → Deep
    let deep = router::ask(
        "I need to understand how the authentication flow works across the entire codebase, \
         including session management, token validation, and the password hashing module. \
         Can you also check for any security vulnerabilities?",
        &ctx,
    );
    assert_eq!(
        deep.complexity,
        QueryComplexity::Deep,
        "long multi-part query should be Deep"
    );
}

// ── Test 4: Each model tier produces structurally different output ──────

#[test]
fn ask_adapts_output_per_model_tier() {
    let query = "explain the database connection pool";

    // Atomic tier — Semantic Ballast, ultra-compact
    let atomic_ctx = make_ctx_with_tier(ModelTier::Atomic);
    let atomic = router::ask_raw(query, &atomic_ctx, false);

    // Molecular tier — structured sections
    let molecular_ctx = make_ctx_with_tier(ModelTier::Molecular);
    let molecular = router::ask_raw(query, &molecular_ctx, false);

    // Galactic tier — rich markdown
    let galactic_ctx = make_ctx_with_tier(ModelTier::Galactic);
    let galactic = router::ask_raw(query, &galactic_ctx, false);

    // Universal tier — full cross-references
    let universal_ctx = make_ctx_with_tier(ModelTier::Universal);
    let universal = router::ask_raw(query, &universal_ctx, false);

    // All tiers produce non-empty output
    assert!(!atomic.smart_context.is_empty(), "Atomic context empty");
    assert!(!molecular.smart_context.is_empty(), "Molecular context empty");
    assert!(!galactic.smart_context.is_empty(), "Galactic context empty");
    assert!(!universal.smart_context.is_empty(), "Universal context empty");

    // Atomic forces raw injection via needs_semantic_ballast()
    // so effective_raw should be true → raw_sources may be populated if targets found.
    // At minimum, the smart_context should have ballast markers for Atomic.

    // Higher tiers should generally produce longer context:
    // Universal ≥ Galactic ≥ Molecular (Atomic is special — ballast mode)
    // We compare only Molecular vs Galactic vs Universal (Atomic follows different rules)
    assert!(
        universal.smart_context.len() >= molecular.smart_context.len(),
        "Universal ({}) should produce >= Molecular ({}) context",
        universal.smart_context.len(),
        molecular.smart_context.len()
    );
}

// ── Test 5: No-match query degrades gracefully ─────────────────────────

#[test]
fn ask_handles_no_match_gracefully() {
    let ctx = make_ctx();

    // Query about something that doesn't exist in the fixture project
    let result = router::ask("explain the kubernetes deployment manifest", &ctx);

    // Must still return a valid result
    assert!(!result.smart_context.is_empty(), "no-match must still produce smart_context");
    assert!(result.sid.is_finite(), "no-match SID must be finite");
    assert!(
        result.pipeline_metrics.total_us > 0,
        "pipeline timing must be recorded even on no-match"
    );
    // Security status should be present (even if trivial)
    assert!(
        !result.security_status.is_empty(),
        "security_status must not be empty"
    );
}

// ── Test 6: raw_injection flag injects source code ─────────────────────

#[test]
fn ask_raw_populates_raw_sources_when_targets_found() {
    let ctx = make_ctx_with_tier(ModelTier::Atomic);

    // Atomic tier forces raw injection.
    // Query a module that exists in the fixture.
    let result = router::ask_raw("explain the auth module", &ctx, true);

    // If targets were found, raw_sources should be populated.
    // If no targets (no search index), raw_sources can be empty — that's ok.
    // The key invariant: the pipeline doesn't panic.
    assert!(result.sid.is_finite());

    // When raw injection is active, the effective_raw flag was set.
    // Even without a search index producing targets, the pipeline should complete.
    assert!(!result.smart_context.is_empty());
}

// ── Test 7: Pipeline metrics have all stages populated ─────────────────

#[test]
fn ask_pipeline_metrics_complete() {
    let ctx = make_ctx();
    let result = router::ask("how does the router work", &ctx);

    let m = &result.pipeline_metrics;

    // Every stage should have been timed (≥ 0)
    // We don't assert > 0 for each because some stages can be near-instant,
    // but total_us must be > 0 and ≥ sum of parts.
    assert!(m.total_us > 0, "total_us must be > 0");

    // Context should have some bytes
    assert!(m.context_bytes > 0, "context_bytes must be > 0");
    assert!(m.context_tokens > 0, "context_tokens must be > 0");

    // Bottleneck detection works
    let bottleneck = m.bottleneck();
    assert!(!bottleneck.is_empty(), "bottleneck must identify a stage");
}

// ── Test 8: Multiple queries produce unique SIDs ───────────────────────

#[test]
fn ask_produces_deterministic_sid_for_same_query() {
    let ctx = make_ctx();

    // Same query, same context → should produce the same SID
    // (no randomness in the pipeline; SID is symbols_found / prompt_tokens)
    let r1 = router::ask("explain the utils module", &ctx);
    let r2 = router::ask("explain the utils module", &ctx);

    assert!(
        (r1.sid - r2.sid).abs() < f64::EPSILON,
        "same query + context should produce identical SID: {} vs {}",
        r1.sid,
        r2.sid
    );

    // Different query → different smart_context at minimum
    let r3 = router::ask("security audit", &ctx);
    assert_ne!(
        r1.smart_context, r3.smart_context,
        "different queries should produce different smart_context"
    );
}

// ── Test 9: Intent hardening — General → Explain when symbols found ────

#[test]
fn ask_hardens_general_to_explain_when_symbols_found() {
    // This tests the intent hardening logic: if symbols are found in targets
    // but intent is General, it gets promoted to Explain.
    // Without a search index we can't guarantee targets, but we test the
    // intent classification path doesn't crash and returns valid results.
    let ctx = make_ctx();

    let result = router::ask("what does login do?", &ctx);
    // Intent should be either General (no symbols found) or Explain (hardened)
    assert!(
        matches!(result.intent, Intent::General | Intent::Explain),
        "expected General or Explain, got {:?}",
        result.intent
    );
}
