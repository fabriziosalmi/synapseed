//! Code security pattern scanner — detects common vulnerability anti-patterns.
//!
//! Scans source code for patterns that indicate potential security issues:
//! - SQL injection: string concatenation in SQL queries
//! - XSS: unescaped HTML/template output
//! - Command injection: shell command string interpolation
//! - Path traversal: unsanitized `..` in file path operations
//!
//! This is a static heuristic scanner (not a full AST analysis), designed for
//! fast scanning with low false-positive rates.

use regex::Regex;
use serde::{Deserialize, Serialize};

/// A detected code security anti-pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodePatternFinding {
    /// Pattern category: "sql_injection", "xss", "command_injection", "path_traversal".
    pub category: String,
    /// Line number (1-based) where the pattern was detected.
    pub line: usize,
    /// The suspicious line content (trimmed).
    pub content: String,
    /// Human-readable explanation of the risk.
    pub risk: String,
    /// Suggested remediation.
    pub suggestion: String,
    /// Confidence: "high" or "medium".
    pub confidence: String,
}

/// Result of a code pattern security scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodePatternReport {
    /// Total lines scanned.
    pub lines_scanned: usize,
    /// Findings grouped by category.
    pub findings: Vec<CodePatternFinding>,
    /// Quick summary status.
    pub status: String,
}

/// Scanner for common code security anti-patterns.
pub struct CodePatternScanner {
    sql_patterns: Vec<Regex>,
    xss_patterns: Vec<Regex>,
    cmd_patterns: Vec<Regex>,
    path_patterns: Vec<Regex>,
}

impl CodePatternScanner {
    /// Create a scanner with the default pattern set.
    pub fn new() -> Self {
        Self {
            sql_patterns: compile_patterns(&[
                // String interpolation in SQL: format!("SELECT ... {}", var)
                r#"(?i)(format!\s*\(\s*"[^"]*(?:SELECT|INSERT|UPDATE|DELETE|DROP|ALTER)[^"]*\{)"#,
                // String concatenation with SQL keywords
                r#"(?i)(?:SELECT|INSERT|UPDATE|DELETE|DROP|ALTER)\s+.*\+\s*[&]?\w+"#,
                // Raw SQL with user input concatenation
                r#"(?i)query\s*\(\s*&?format!\s*\("#,
            ]),
            xss_patterns: compile_patterns(&[
                // innerHTML/outerHTML assignment with variable (not string literal)
                r#"\.(innerHTML|outerHTML)\s*=\s*[^"'\s;]"#,
                // document.write with variable
                r#"document\.write(ln)?\s*\("#,
                // insertAdjacentHTML with variable
                r#"\.insertAdjacentHTML\s*\("#,
                // Unescaped template literal in HTML context
                r#"<[a-zA-Z][^>]*\$\{[^}]*\}[^>]*>"#,
                // format_args with HTML tags and variables
                r#"(?i)format!\s*\(\s*"[^"]*<[a-z]+[^"]*\{[^"]*""#,
            ]),
            cmd_patterns: compile_patterns(&[
                // Command::new with format! argument
                r#"Command::new\s*\(\s*&?format!\s*\("#,
                // shell command with interpolation
                r#"(?i)(?:exec|system|popen|spawn)\s*\(\s*&?format!\s*\("#,
                // sh -c with format string
                r#"(?i)"sh"\s*,\s*"-c"\s*,\s*&?format!\s*\("#,
                // Backtick command substitution with variable
                r#"`[^`]*\$\{[^}]*\}[^`]*`"#,
                // v4.17.1: .arg() with format! or variable after sh -c (cross-line pattern)
                r#"\.arg\s*\(\s*&?format!\s*\("#,
                // v4.17.1: Python subprocess with f-string or format
                r#"(?i)(?:subprocess\.(?:call|run|Popen)|os\.(?:system|popen))\s*\(\s*f?["']"#,
                // v4.17.1: Python eval/exec with variable
                r#"(?i)(?:eval|exec)\s*\(\s*(?:f["']|\w+\s*\+)"#,
            ]),
            path_patterns: compile_patterns(&[
                // Path/PathBuf join with unvalidated user input
                r#"(?:path|root|dir|base|prefix)\S*\.join\s*\(\s*&?\w+\)"#,
                // Direct ".." in path construction
                r#"Path::new\s*\(\s*&?format!\s*\("#,
                // Filesystem ops with format! paths
                r#"(?:read_to_string|write|remove_file|create_dir)\s*\(\s*&?format!\s*\("#,
            ]),
        }
    }

    /// Create a scanner filtered by active categories.
    /// Empty `categories` = all active (same as `new()`).
    pub fn from_categories(categories: &[String]) -> Self {
        if categories.is_empty() {
            return Self::new();
        }
        let mut scanner = Self::new();
        if !categories.iter().any(|c| c == "sql_injection") {
            scanner.sql_patterns.clear();
        }
        if !categories.iter().any(|c| c == "xss") {
            scanner.xss_patterns.clear();
        }
        if !categories.iter().any(|c| c == "command_injection") {
            scanner.cmd_patterns.clear();
        }
        if !categories.iter().any(|c| c == "path_traversal") {
            scanner.path_patterns.clear();
        }
        scanner
    }

    /// Scan source code for security anti-patterns.
    pub fn scan(&self, content: &str) -> CodePatternReport {
        let lines: Vec<&str> = content.lines().collect();
        let mut findings = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            let line_num = i + 1;

            // Skip comments and empty lines
            if trimmed.is_empty()
                || trimmed.starts_with("//")
                || trimmed.starts_with('#')
                || trimmed.starts_with('*')
            {
                continue;
            }

            // SQL injection patterns
            for pat in &self.sql_patterns {
                if pat.is_match(trimmed) {
                    findings.push(CodePatternFinding {
                        category: "sql_injection".to_string(),
                        line: line_num,
                        content: truncate(trimmed, 120),
                        risk: "SQL query constructed with string interpolation — potential SQL injection."
                            .to_string(),
                        suggestion: "Use parameterized queries (e.g., sqlx::query!() or prepared statements) instead of string formatting."
                            .to_string(),
                        confidence: "high".to_string(),
                    });
                    break; // one finding per line per category
                }
            }

            // XSS patterns
            for pat in &self.xss_patterns {
                if pat.is_match(trimmed) {
                    findings.push(CodePatternFinding {
                        category: "xss".to_string(),
                        line: line_num,
                        content: truncate(trimmed, 120),
                        risk: "HTML output with unsanitized input — potential cross-site scripting."
                            .to_string(),
                        suggestion: "Escape HTML entities before rendering, or use a template engine with auto-escaping."
                            .to_string(),
                        confidence: "medium".to_string(),
                    });
                    break;
                }
            }

            // Command injection patterns
            for pat in &self.cmd_patterns {
                if pat.is_match(trimmed) {
                    findings.push(CodePatternFinding {
                        category: "command_injection".to_string(),
                        line: line_num,
                        content: truncate(trimmed, 120),
                        risk: "Shell command constructed with string interpolation — potential command injection."
                            .to_string(),
                        suggestion: "Use Command::new() with .arg() for each argument separately. Never interpolate user input into shell strings."
                            .to_string(),
                        confidence: "high".to_string(),
                    });
                    break;
                }
            }

            // Path traversal patterns
            for pat in &self.path_patterns {
                if pat.is_match(trimmed) {
                    findings.push(CodePatternFinding {
                        category: "path_traversal".to_string(),
                        line: line_num,
                        content: truncate(trimmed, 120),
                        risk: "File path constructed from potentially unvalidated input — possible path traversal."
                            .to_string(),
                        suggestion: "Validate and canonicalize paths before use. Reject inputs containing `..` or absolute paths."
                            .to_string(),
                        confidence: "medium".to_string(),
                    });
                    break;
                }
            }
        }

        let status = if findings.is_empty() {
            "CLEAN: No security anti-patterns detected.".to_string()
        } else {
            let high = findings.iter().filter(|f| f.confidence == "high").count();
            let medium = findings.iter().filter(|f| f.confidence == "medium").count();
            format!(
                "ALERT: {} finding(s) — {} high confidence, {} medium confidence",
                findings.len(),
                high,
                medium
            )
        };

        CodePatternReport {
            lines_scanned: lines.len(),
            findings,
            status,
        }
    }
}

impl Default for CodePatternScanner {
    fn default() -> Self {
        Self::new()
    }
}

fn compile_patterns(patterns: &[&str]) -> Vec<Regex> {
    patterns
        .iter()
        .filter_map(|p| Regex::new(p).ok())
        .collect()
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        // Find the nearest char boundary at or before `max` to avoid panicking
        // on multi-byte UTF-8 sequences.
        let end = s.floor_char_boundary(max);
        format!("{}...", &s[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_sql_injection() {
        let scanner = CodePatternScanner::new();
        let code = r#"
let query = format!("SELECT * FROM users WHERE name = '{}'", user_input);
"#;
        let report = scanner.scan(code);
        assert!(
            report.findings.iter().any(|f| f.category == "sql_injection"),
            "Expected SQL injection finding"
        );
    }

    #[test]
    fn test_detect_command_injection() {
        let scanner = CodePatternScanner::new();
        let code = r#"
let output = Command::new(&format!("grep {} /var/log/app.log", user_query));
"#;
        let report = scanner.scan(code);
        assert!(
            report.findings.iter().any(|f| f.category == "command_injection"),
            "Expected command injection finding"
        );
    }

    #[test]
    fn test_clean_code() {
        let scanner = CodePatternScanner::new();
        let code = r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#;
        let report = scanner.scan(code);
        assert!(report.findings.is_empty(), "Expected no findings for clean code");
        assert!(report.status.starts_with("CLEAN"));
    }

    #[test]
    fn test_detect_xss() {
        let scanner = CodePatternScanner::new();
        let code = r#"
document.write(userInput);
element.innerHTML = data;
"#;
        let report = scanner.scan(code);
        assert!(
            report.findings.iter().any(|f| f.category == "xss"),
            "Expected XSS finding"
        );
    }
}
