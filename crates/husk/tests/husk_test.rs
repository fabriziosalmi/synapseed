//! Integration tests for the synapseed-husk crate.
//!
//! Tests DLP secret detection, false-positive whitelisting, redaction,
//! code pattern detection, and SecurityGuard behavior.

use synapseed_core::policy::{PolicyAction, SecurityPolicy};
use synapseed_husk::guard::SecurityGuard;
use synapseed_husk::patterns::CodePatternScanner;

// ── DLP Secret Detection ────────────────────────────────────────────

#[test]
fn detect_aws_access_key() {
    let guard = SecurityGuard::with_defaults();
    let content = "aws_key = AKIAIOSFODNN7EXAMPLE";
    let result = guard.check(content);
    assert!(result.is_err(), "AWS access key should be detected");
}

#[test]
fn detect_github_personal_token() {
    let guard = SecurityGuard::with_defaults();
    let content = "token = ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmn";
    let result = guard.check(content);
    assert!(result.is_err(), "GitHub personal access token should be detected");
}

#[test]
fn detect_github_oauth_token() {
    let guard = SecurityGuard::with_defaults();
    let content = "gho_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmn";
    let result = guard.check(content);
    assert!(result.is_err(), "GitHub OAuth token should be detected");
}

#[test]
fn detect_generic_password_assignment() {
    let guard = SecurityGuard::with_defaults();
    let content = r#"password = "supersecretvalue123""#;
    let result = guard.check(content);
    assert!(result.is_err(), "Generic password assignment should be detected");
}

#[test]
fn detect_generic_api_key_assignment() {
    let guard = SecurityGuard::with_defaults();
    let content = r#"api_key = "sk-proj-abcdefghij12345678""#;
    let result = guard.check(content);
    assert!(result.is_err(), "Generic api_key assignment should be detected");
}

#[test]
fn detect_private_key_header() {
    let guard = SecurityGuard::with_defaults();
    let content = "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAK...";
    let result = guard.check(content);
    assert!(result.is_err(), "RSA private key header should be detected");
}

#[test]
fn clean_code_passes_check() {
    let guard = SecurityGuard::with_defaults();
    let content = r#"
fn main() {
    let x = 42;
    println!("Hello, world! {}", x);
}
"#;
    let result = guard.check(content);
    assert!(result.is_ok(), "Clean code should pass DLP check");
}

// ── False Positive Whitelist ────────────────────────────────────────

#[test]
fn whitelist_cancellation_token_type_annotation() {
    let guard = SecurityGuard::with_defaults();
    // This looks like "token = Something" but is a type annotation, not a secret
    let content = "let token = CancellationToken::new();";
    let result = guard.check(content);
    assert!(
        result.is_ok(),
        "CancellationToken type assignment should be whitelisted, got: {:?}",
        result
    );
}

#[test]
fn whitelist_shutdown_token_pattern() {
    let guard = SecurityGuard::with_defaults();
    let content = "let shutdown_token = ctx.shutdown_token();";
    let result = guard.check(content);
    assert!(
        result.is_ok(),
        "shutdown_token pattern should be whitelisted, got: {:?}",
        result
    );
}

// ── Redaction ───────────────────────────────────────────────────────

#[test]
fn redact_replaces_aws_key_with_placeholder() {
    let guard = SecurityGuard::with_defaults();
    let content = "key = AKIAIOSFODNN7EXAMPLE";
    let redacted = guard.redact(content);
    assert!(
        redacted.contains("[REDACTED]"),
        "Redacted output should contain [REDACTED], got: {}",
        redacted
    );
    assert!(
        !redacted.contains("AKIAIOSFODNN7EXAMPLE"),
        "Redacted output should not contain the original secret"
    );
}

#[test]
fn redact_preserves_non_secret_content() {
    let guard = SecurityGuard::with_defaults();
    let content = "safe_var = 42\nkey = AKIAIOSFODNN7EXAMPLE\nother = hello";
    let redacted = guard.redact(content);
    assert!(redacted.contains("safe_var = 42"), "Non-secret content should be preserved");
    assert!(redacted.contains("other = hello"), "Non-secret content should be preserved");
    assert!(redacted.contains("[REDACTED]"), "Secret should be redacted");
}

// ── Sanitize: fail_closed vs open ───────────────────────────────────

#[test]
fn sanitize_fail_closed_blocks_on_secret() {
    // Default SecurityGuard uses fail_closed=true
    let guard = SecurityGuard::with_defaults();
    let content = "password = \"my_super_secret_pw\"";
    let result = guard.sanitize(content);
    assert!(
        result.is_err(),
        "fail_closed=true should return Err on secret detection"
    );
}

#[test]
fn sanitize_fail_open_redacts_instead_of_blocking() {
    let policy = SecurityPolicy {
        dlp_rules: Vec::new(), // uses defaults
        command_rules: Vec::new(),
        fail_closed: false,
        dlp_whitelist: Vec::new(),
    };
    let guard = SecurityGuard::from_policy(&policy);
    let content = "password = \"my_super_secret_pw\"";
    let result = guard.sanitize(content);
    assert!(
        result.is_ok(),
        "fail_closed=false should return Ok with redacted content"
    );
    let redacted = result.unwrap();
    assert!(
        redacted.contains("[REDACTED]"),
        "Redacted content should contain [REDACTED], got: {}",
        redacted
    );
}

#[test]
fn check_is_non_destructive() {
    let guard = SecurityGuard::with_defaults();
    let content = "password = \"my_super_secret_pw\"";
    // check() should detect the violation but not modify the content
    let result = guard.check(content);
    assert!(result.is_err());
    // The original content string should still be intact
    assert!(content.contains("my_super_secret_pw"));
}

// ── Code Pattern Detection ──────────────────────────────────────────

#[test]
fn detect_sql_injection_pattern() {
    let scanner = CodePatternScanner::new();
    let code = r#"let q = format!("SELECT * FROM users WHERE id = '{}'", user_id);"#;
    let report = scanner.scan(code);
    assert!(
        report.findings.iter().any(|f| f.category == "sql_injection"),
        "SQL injection pattern should be detected, findings: {:?}",
        report.findings
    );
}

#[test]
fn detect_xss_innerhtml_pattern() {
    let scanner = CodePatternScanner::new();
    let code = "element.innerHTML = userInput;";
    let report = scanner.scan(code);
    assert!(
        report.findings.iter().any(|f| f.category == "xss"),
        "XSS innerHTML pattern should be detected"
    );
}

#[test]
fn detect_command_injection_pattern() {
    let scanner = CodePatternScanner::new();
    let code = r#"Command::new(&format!("rm -rf {}", user_path));"#;
    let report = scanner.scan(code);
    assert!(
        report.findings.iter().any(|f| f.category == "command_injection"),
        "Command injection pattern should be detected"
    );
}

#[test]
fn detect_path_traversal_pattern() {
    let scanner = CodePatternScanner::new();
    let code = r#"let data = std::fs::read_to_string(&format!("/data/{}", filename));"#;
    let report = scanner.scan(code);
    assert!(
        report.findings.iter().any(|f| f.category == "path_traversal"),
        "Path traversal pattern should be detected"
    );
}

#[test]
fn clean_code_has_zero_findings() {
    let scanner = CodePatternScanner::new();
    let code = r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn main() {
    let result = add(1, 2);
    println!("Result: {}", result);
}
"#;
    let report = scanner.scan(code);
    assert!(report.findings.is_empty(), "Clean code should have no findings");
    assert!(report.status.starts_with("CLEAN"));
}

#[test]
fn code_pattern_skips_comments() {
    let scanner = CodePatternScanner::new();
    let code = r#"
// format!("SELECT * FROM users WHERE id = '{}'", user_id);
// This is a comment explaining SQL injection risks
fn safe_function() {}
"#;
    let report = scanner.scan(code);
    assert!(
        report.findings.is_empty(),
        "Commented-out code should not produce findings"
    );
}

#[test]
fn code_pattern_category_filter() {
    let scanner = CodePatternScanner::from_categories(&["sql_injection".to_string()]);
    // This has both XSS and SQL injection, but we only enabled sql_injection
    let code = r#"
element.innerHTML = userInput;
let q = format!("SELECT * FROM users WHERE id = '{}'", uid);
"#;
    let report = scanner.scan(code);
    assert!(
        report.findings.iter().all(|f| f.category == "sql_injection"),
        "Only sql_injection findings should appear when filtering by category"
    );
}

#[test]
fn code_pattern_report_counts_severity() {
    let scanner = CodePatternScanner::new();
    let code = r#"
let q = format!("SELECT * FROM users WHERE id = '{}'", uid);
element.innerHTML = data;
Command::new(&format!("rm {}", path));
"#;
    let report = scanner.scan(code);
    assert!(!report.findings.is_empty(), "Should detect multiple findings");
    assert!(
        report.status.contains("ALERT"),
        "Status should be ALERT, got: {}",
        report.status
    );
    // SQL injection and command injection are "high" confidence
    let high_count = report
        .findings
        .iter()
        .filter(|f| f.confidence == "high")
        .count();
    assert!(high_count >= 1, "Should have at least 1 high-confidence finding");
}
