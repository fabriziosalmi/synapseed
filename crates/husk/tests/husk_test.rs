//! Integration tests for the synapseed-husk crate.
//!
//! Tests DLP secret detection, false-positive whitelisting, redaction,
//! code pattern detection, and SecurityGuard behavior.

use synapseed_core::policy::SecurityPolicy;
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

// ── v4.17.1: URI Credential Detection ────────────────────────────────

#[test]
fn detect_postgres_uri_credentials() {
    let guard = SecurityGuard::with_defaults();
    let content = r#"DATABASE_URL = "postgresql://admin:s3cret_pass@db.example.com:5432/prod""#;
    let result = guard.check(content);
    assert!(result.is_err(), "PostgreSQL URI with embedded password should be detected");
}

#[test]
fn detect_mongodb_uri_credentials() {
    let guard = SecurityGuard::with_defaults();
    let content = r#"MONGO_URL = "mongodb+srv://root:hunter2@cluster0.abc123.mongodb.net/test""#;
    let result = guard.check(content);
    assert!(result.is_err(), "MongoDB URI with embedded password should be detected");
}

#[test]
fn detect_redis_uri_credentials() {
    let guard = SecurityGuard::with_defaults();
    let content = r#"REDIS_URL = "redis://default:mypassword@redis.example.com:6379""#;
    let result = guard.check(content);
    assert!(result.is_err(), "Redis URI with embedded password should be detected");
}

#[test]
fn clean_uri_without_credentials_passes() {
    let guard = SecurityGuard::with_defaults();
    let content = r#"DATABASE_URL = "postgresql://db.example.com:5432/prod""#;
    let result = guard.check(content);
    assert!(result.is_ok(), "URI without credentials should pass DLP check");
}

#[test]
fn detect_jwt_token() {
    let guard = SecurityGuard::with_defaults();
    // Valid JWT structure: header.payload.signature (all base64url)
    let content = "token = eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
    let result = guard.check(content);
    assert!(result.is_err(), "JWT token should be detected");
}

#[test]
fn detect_slack_webhook_url() {
    let guard = SecurityGuard::with_defaults();
    // Build URL at runtime to avoid triggering GitHub push protection
    let url = format!(
        "WEBHOOK = https://hooks.slack.com/services/{}/{}/{}",
        "T00000000", "B00000000", "XXXXXXXXXXXXXXXXXXXXXXXX"
    );
    let result = guard.check(&url);
    assert!(result.is_err(), "Slack webhook URL should be detected");
}

#[test]
fn detect_expanded_generic_secret_keywords() {
    let guard = SecurityGuard::with_defaults();

    let cases = vec![
        (r#"access_key = "AKIAIOSFODNN7testtest""#, "access_key"),
        (r#"client_secret = "abcdefghij1234567890""#, "client_secret"),
        (r#"auth_token = "tok_live_1234567890abcdef""#, "auth_token"),
        (r#"bearer = "eyToken1234567890abcdef""#, "bearer"),
        (r#"signing_key = "whsec_abcdefghijklmnop""#, "signing_key"),
    ];

    for (content, label) in cases {
        let result = guard.check(content);
        assert!(result.is_err(), "{} assignment should be detected as secret", label);
    }
}

// ── v4.17.1: Command Injection .arg(format!()) ──────────────────────

#[test]
fn detect_command_injection_arg_format() {
    let scanner = CodePatternScanner::new();
    let code = r#"Command::new("sh").arg("-c").arg(&format!("ls {}", user_input));"#;
    let report = scanner.scan(code);
    assert!(
        report.findings.iter().any(|f| f.category == "command_injection"),
        "Command injection via .arg(format!()) should be detected, findings: {:?}",
        report.findings
    );
}

#[test]
fn detect_python_subprocess_f_string() {
    let scanner = CodePatternScanner::new();
    let code = r#"subprocess.call(f"rm -rf {path}")"#;
    let report = scanner.scan(code);
    assert!(
        report.findings.iter().any(|f| f.category == "command_injection"),
        "Python subprocess f-string should be detected, findings: {:?}",
        report.findings
    );
}

#[test]
fn detect_python_eval_with_variable() {
    let scanner = CodePatternScanner::new();
    let code = r#"eval(user_input + ".method()")"#;
    let report = scanner.scan(code);
    assert!(
        report.findings.iter().any(|f| f.category == "command_injection"),
        "Python eval with variable concatenation should be detected, findings: {:?}",
        report.findings
    );
}

// ── v4.17.2 (W4/W10): Aggressive heuristic .arg(&var) ──────────────

#[test]
fn detect_command_injection_arg_ref_variable() {
    let scanner = CodePatternScanner::new();
    // Cross-line pattern: let cmd = format!("..."); Command::new("sh").arg(&cmd)
    let code = r#"Command::new("sh").arg("-c").arg(&cmd)"#;
    let report = scanner.scan(code);
    assert!(
        report.findings.iter().any(|f| f.category == "command_injection"),
        "Dynamic .arg(&var) should trigger heuristic warning, findings: {:?}",
        report.findings
    );
}

#[test]
fn detect_command_injection_args_ref_variable() {
    let scanner = CodePatternScanner::new();
    let code = r#"Command::new("ls").args(&user_args)"#;
    let report = scanner.scan(code);
    assert!(
        report.findings.iter().any(|f| f.category == "command_injection"),
        "Dynamic .args(&var) should trigger heuristic warning, findings: {:?}",
        report.findings
    );
}

#[test]
fn no_false_positive_arg_string_literal() {
    let scanner = CodePatternScanner::new();
    // .arg("--flag") should NOT trigger — it's a safe literal
    let code = r#"Command::new("ls").arg("--all").arg("-l")"#;
    let report = scanner.scan(code);
    assert!(
        !report.findings.iter().any(|f| f.category == "command_injection"),
        "Static .arg(\"literal\") should NOT trigger, findings: {:?}",
        report.findings
    );
}

// ── v4.17.2 (W9): Generic URI protocol detection ───────────────────

#[test]
fn detect_custom_protocol_uri_credentials() {
    let guard = SecurityGuard::with_defaults();
    // Protocols not in the old explicit list
    let cases = [
        ("http://admin:pass123@host.com", "http"),
        ("https://user:secret@api.example.com", "https"),
        ("amqps://rabbit:mq_pass@broker.io", "amqps"),
        ("nats://user:pass@nats-server:4222", "nats"),
        ("cockroachdb://root:pw@crdb:26257", "cockroachdb"),
    ];
    for (uri, protocol) in &cases {
        let result = guard.check(uri);
        assert!(
            result.is_err(),
            "{protocol}:// URI with credentials should be detected: {uri}"
        );
    }
}
