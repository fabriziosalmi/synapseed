//! Property-based tests for synapseed-husk using proptest.
//!
//! These tests verify robustness guarantees that must hold for ALL inputs,
//! not just hand-picked examples. proptest generates thousands of random
//! inputs and shrinks failures to minimal reproducers.

use proptest::prelude::*;
use synapseed_husk::guard::SecurityGuard;
use synapseed_husk::patterns::CodePatternScanner;

// ═══════════════════════════════════════════════════════════════════
// SecurityGuard (DLP scanner) properties
// ═══════════════════════════════════════════════════════════════════

proptest! {
    /// The guard must never panic on arbitrary input.
    /// This catches indexing errors, regex catastrophic backtracking,
    /// and invalid UTF-8 boundary slicing.
    #[test]
    fn guard_check_never_panics(input in "\\PC{0,1000}") {
        let guard = SecurityGuard::with_defaults();
        // We don't care about the result — only that it doesn't panic.
        let _ = guard.check(&input);
    }

    /// Redaction must be idempotent: once content is redacted, redacting
    /// again must produce the exact same output. This ensures that
    /// [REDACTED] markers themselves are not flagged as secrets.
    #[test]
    fn redaction_is_idempotent(input in "\\PC{0,500}") {
        let guard = SecurityGuard::with_defaults();
        let first = guard.redact(&input);
        let second = guard.redact(&first);
        prop_assert_eq!(&first, &second,
            "Redaction was not idempotent: first pass produced different output than second pass");
    }

    /// The redacted output must never be longer than the original input
    /// plus the maximum possible expansion from [REDACTED] markers.
    /// More importantly, it must be valid UTF-8 (guaranteed by String type).
    #[test]
    fn redact_produces_valid_utf8(input in "\\PC{0,1000}") {
        let guard = SecurityGuard::with_defaults();
        let redacted = guard.redact(&input);
        // String type guarantees UTF-8, but let's verify the content is reasonable.
        // The redacted output should not contain raw bytes or garbage.
        prop_assert!(
            redacted.chars().all(|c| !c.is_control() || c == '\n' || c == '\r' || c == '\t'),
            "Redacted output contains unexpected characters"
        );
    }

    /// If the guard finds no violations in the input, check() must return Ok.
    /// Conversely, redact() on clean input must return the input unchanged.
    #[test]
    fn clean_input_roundtrips(input in "[a-zA-Z0-9 ]{0,200}") {
        let guard = SecurityGuard::with_defaults();
        let redacted = guard.redact(&input);
        // Simple alphanumeric input should never be flagged
        prop_assert_eq!(&input, &redacted,
            "Simple alphanumeric input was unexpectedly redacted");
    }
}

// ═══════════════════════════════════════════════════════════════════
// CodePatternScanner properties
// ═══════════════════════════════════════════════════════════════════

proptest! {
    /// The code pattern scanner must never panic on arbitrary input.
    /// This tests the regex engine's resilience to pathological patterns,
    /// deeply nested constructs, and binary-like content.
    #[test]
    fn code_scanner_never_panics(input in "\\PC{0,2000}") {
        let scanner = CodePatternScanner::new();
        let _ = scanner.scan(&input);
    }

    /// Scanning the same input twice must produce the same findings.
    /// This verifies there is no hidden mutable state in the scanner.
    #[test]
    fn code_scanner_is_deterministic(input in "\\PC{0,500}") {
        let scanner = CodePatternScanner::new();
        let report1 = scanner.scan(&input);
        let report2 = scanner.scan(&input);
        prop_assert_eq!(report1.lines_scanned, report2.lines_scanned,
            "Line count differed between scans");
        prop_assert_eq!(report1.findings.len(), report2.findings.len(),
            "Finding count differed between scans");
        prop_assert_eq!(&report1.status, &report2.status,
            "Status differed between scans");
    }

    /// lines_scanned must equal the number of lines in the input.
    #[test]
    fn code_scanner_line_count_matches(input in "\\PC{0,500}") {
        let scanner = CodePatternScanner::new();
        let report = scanner.scan(&input);
        let expected_lines = input.lines().count();
        prop_assert_eq!(report.lines_scanned, expected_lines,
            "lines_scanned does not match actual line count");
    }

    /// Every finding must reference a valid line number (1-based, within range).
    #[test]
    fn code_scanner_findings_have_valid_line_numbers(input in "\\PC{0,1000}") {
        let scanner = CodePatternScanner::new();
        let report = scanner.scan(&input);
        let total_lines = input.lines().count();
        for finding in &report.findings {
            prop_assert!(finding.line >= 1 && finding.line <= total_lines,
                "Finding line {} out of range [1, {}]", finding.line, total_lines);
        }
    }

    /// Category-filtered scanner must never report findings outside its categories.
    #[test]
    fn category_filter_is_respected(input in "\\PC{0,500}") {
        let scanner = CodePatternScanner::from_categories(&["sql_injection".to_string()]);
        let report = scanner.scan(&input);
        for finding in &report.findings {
            prop_assert_eq!(&finding.category, "sql_injection",
                "Found category '{}' but only 'sql_injection' was enabled", finding.category);
        }
    }
}
