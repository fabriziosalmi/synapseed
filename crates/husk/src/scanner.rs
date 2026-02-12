use aho_corasick::AhoCorasick;
use regex::Regex;
use synapseed_core::policy::DlpRule;
use tracing::warn;

/// A compiled DLP scanner that detects secrets and sensitive patterns.
///
/// Uses a two-pass approach:
/// 1. Aho-Corasick automaton for static terms (GB/s throughput)
/// 2. Regex patterns for structured secrets (API keys, tokens)
pub struct DlpScanner {
    /// Fast multi-pattern matcher for static terms
    static_matcher: Option<AhoCorasick>,
    static_terms: Vec<String>,
    /// Compiled regex patterns for structured secrets
    regex_patterns: Vec<CompiledPattern>,
}

struct CompiledPattern {
    name: String,
    regex: Regex,
}

impl DlpScanner {
    /// Build a scanner from a list of DLP rules.
    pub fn from_rules(rules: &[DlpRule]) -> Self {
        let mut static_terms = Vec::new();
        let mut regex_patterns = Vec::new();

        for rule in rules {
            // If the pattern looks like a regex (contains metacharacters), compile it
            if rule.pattern.contains('(')
                || rule.pattern.contains('[')
                || rule.pattern.contains('\\')
                || rule.pattern.contains('+')
                || rule.pattern.contains('*')
            {
                match Regex::new(&rule.pattern) {
                    Ok(re) => regex_patterns.push(CompiledPattern {
                        name: rule.name.clone(),
                        regex: re,
                    }),
                    Err(e) => warn!(
                        rule = %rule.name,
                        error = %e,
                        "Failed to compile DLP regex, skipping"
                    ),
                }
            } else {
                static_terms.push(rule.pattern.clone());
            }
        }

        let static_matcher = if static_terms.is_empty() {
            None
        } else {
            AhoCorasick::new(&static_terms).ok()
        };

        Self {
            static_matcher,
            static_terms,
            regex_patterns,
        }
    }

    /// Create a scanner with sensible defaults for common secret patterns.
    pub fn with_defaults() -> Self {
        let default_rules = vec![
            DlpRule {
                name: "aws_key".into(),
                pattern: r"AKIA[0-9A-Z]{16}".into(),
                action: synapseed_core::policy::PolicyAction::Redact,
            },
            DlpRule {
                name: "github_token".into(),
                pattern: r"gh[pousr]_[A-Za-z0-9_]{36,}".into(),
                action: synapseed_core::policy::PolicyAction::Redact,
            },
            DlpRule {
                name: "generic_secret".into(),
                pattern: r#"(?i)(password|secret|token|api_key)\s*[:=]\s*["']?[^\s"']{8,}"#.into(),
                action: synapseed_core::policy::PolicyAction::Redact,
            },
            DlpRule {
                name: "private_key".into(),
                pattern: r"-----BEGIN (RSA |EC |DSA )?PRIVATE KEY-----".into(),
                action: synapseed_core::policy::PolicyAction::Redact,
            },
        ];

        Self::from_rules(&default_rules)
    }

    /// Scan content and return all findings.
    pub fn scan(&self, content: &str) -> Vec<Finding> {
        let mut findings = Vec::new();

        // Pass 1: Aho-Corasick static terms
        if let Some(ref matcher) = self.static_matcher {
            for mat in matcher.find_iter(content) {
                findings.push(Finding {
                    rule_name: format!("static_term:{}", &self.static_terms[mat.pattern().as_usize()]),
                    start: mat.start(),
                    end: mat.end(),
                    matched_text: content[mat.start()..mat.end()].to_string(),
                });
            }
        }

        // Pass 2: Regex patterns
        for pat in &self.regex_patterns {
            for mat in pat.regex.find_iter(content) {
                findings.push(Finding {
                    rule_name: pat.name.clone(),
                    start: mat.start(),
                    end: mat.end(),
                    matched_text: mat.as_str().to_string(),
                });
            }
        }

        findings
    }

    /// Scan and redact: replace all findings with [REDACTED].
    pub fn redact(&self, content: &str) -> (String, Vec<Finding>) {
        let findings = self.scan(content);

        if findings.is_empty() {
            return (content.to_string(), findings);
        }

        // Sort by start position, then replace from end to start to preserve offsets
        let mut sorted = findings.clone();
        sorted.sort_by(|a, b| b.start.cmp(&a.start));

        let mut result = content.to_string();
        for finding in &sorted {
            result.replace_range(finding.start..finding.end, "[REDACTED]");
        }

        (result, findings)
    }
}

/// A single DLP finding within scanned content.
#[derive(Debug, Clone)]
pub struct Finding {
    pub rule_name: String,
    pub start: usize,
    pub end: usize,
    pub matched_text: String,
}
