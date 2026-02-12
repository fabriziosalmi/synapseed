//! Property-based tests for the Sentinel command gatekeeper.
//!
//! These tests verify that the Sentinel never panics on arbitrary input
//! and that its evaluation is deterministic and consistent.

use proptest::prelude::*;
use synapseed_core::policy::{CommandRule, PolicyAction, SecurityPolicy};
use synapseed_root::sentinel::Sentinel;

/// Build a sentinel with the default ruleset.
fn default_sentinel() -> Sentinel {
    Sentinel::with_defaults().expect("default rules should compile")
}

/// Build a sentinel from custom rules.
fn sentinel_from_rules(rules: Vec<CommandRule>, fail_closed: bool) -> Sentinel {
    let policy = SecurityPolicy {
        dlp_rules: Vec::new(),
        command_rules: rules,
        fail_closed,
        dlp_whitelist: Vec::new(),
    };
    Sentinel::from_policy(&policy).expect("custom rules should compile")
}

proptest! {
    /// The sentinel must never panic when evaluating any arbitrary string.
    /// This tests regex resilience to pathological input, deeply nested
    /// repetitions, null bytes, control characters, and unicode.
    #[test]
    fn sentinel_never_panics(input in "\\PC{0,1000}") {
        let sentinel = default_sentinel();
        // We only care that it doesn't panic — the result can be Ok or Err.
        let _ = sentinel.evaluate(&input);
    }

    /// Evaluation must be deterministic: the same input must always produce
    /// the same verdict. This ensures no hidden mutable state or randomness.
    #[test]
    fn evaluation_is_deterministic(input in "\\PC{0,500}") {
        let sentinel = default_sentinel();
        let result1 = sentinel.evaluate(&input);
        let result2 = sentinel.evaluate(&input);

        match (&result1, &result2) {
            (Ok(a), Ok(b)) => prop_assert_eq!(
                std::mem::discriminant(a),
                std::mem::discriminant(b),
                "Same input produced different actions"
            ),
            (Err(_), Err(_)) => {
                // Both denied — consistent.
            }
            _ => prop_assert!(false,
                "Same input produced Ok and Err on different evaluations"),
        }
    }

    /// Under fail-closed with no rules, every command must be denied.
    /// This is a critical security invariant.
    #[test]
    fn fail_closed_empty_rules_denies_everything(input in "\\PC{1,500}") {
        let sentinel = sentinel_from_rules(vec![], true);
        let result = sentinel.evaluate(&input);
        prop_assert!(result.is_err(),
            "Fail-closed with no rules should deny all input, but got Ok for: {:?}",
            input);
    }

    /// Under fail-open with no rules, every command must be allowed.
    #[test]
    fn fail_open_empty_rules_allows_everything(input in "\\PC{1,500}") {
        let sentinel = sentinel_from_rules(vec![], false);
        let result = sentinel.evaluate(&input);
        prop_assert!(result.is_ok(),
            "Fail-open with no rules should allow all input, but got Err for: {:?}",
            input);
    }

    /// Leading/trailing whitespace must not change the semantic evaluation.
    /// The sentinel trims input, so " cmd " and "cmd" must yield the same result.
    #[test]
    fn whitespace_invariance(input in "[a-zA-Z0-9/\\-]{1,100}") {
        let sentinel = default_sentinel();
        let padded = format!("  {}  ", input);
        let result_trimmed = sentinel.evaluate(&input);
        let result_padded = sentinel.evaluate(&padded);

        match (&result_trimmed, &result_padded) {
            (Ok(a), Ok(b)) => prop_assert_eq!(
                std::mem::discriminant(a),
                std::mem::discriminant(b),
                "Whitespace-padded input got different action"
            ),
            (Err(_), Err(_)) => {
                // Both denied — consistent.
            }
            _ => prop_assert!(false,
                "Whitespace padding changed the verdict for: {:?}", input),
        }
    }

    /// An explicit Allow rule must always allow the matching input,
    /// regardless of fail-closed setting.
    #[test]
    fn explicit_allow_overrides_fail_closed(input in "[a-z]{1,20}") {
        let sentinel = sentinel_from_rules(
            vec![CommandRule {
                pattern: r"^[a-z]+$".into(),
                action: PolicyAction::Allow,
                description: Some("Allow lowercase commands".into()),
            }],
            true, // fail-closed
        );
        let result = sentinel.evaluate(&input);
        prop_assert!(result.is_ok(),
            "Explicit allow rule should override fail-closed for: {:?}", input);
    }
}
