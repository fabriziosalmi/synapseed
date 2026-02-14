use aho_corasick::AhoCorasick;
use regex::Regex;
use synapseed_core::policy::DlpRule;
use tracing::warn;

/// A compiled DLP scanner that detects secrets and sensitive patterns.
///
/// Uses a two-pass approach:
/// 1. Aho-Corasick automaton for static terms (GB/s throughput)
/// 2. Regex patterns for structured secrets (API keys, tokens)
///
/// An optional whitelist suppresses false-positive findings: if the matched
/// text contains any compiled whitelist pattern, the finding is dropped.
pub(crate) struct DlpScanner {
    /// Fast multi-pattern matcher for static terms
    static_matcher: Option<AhoCorasick>,
    static_terms: Vec<String>,
    /// Compiled regex patterns for structured secrets
    regex_patterns: Vec<CompiledPattern>,
    /// Compiled whitelist patterns (suppress findings whose matched text matches)
    whitelist: Vec<Regex>,
}

struct CompiledPattern {
    name: String,
    regex: Regex,
}

impl DlpScanner {
    /// Build a scanner from a list of DLP rules.
    pub(crate) fn from_rules(rules: &[DlpRule]) -> Self {
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
            whitelist: Vec::new(),
        }
    }

    /// Create a scanner with sensible defaults for common secret patterns.
    pub(crate) fn with_defaults() -> Self {
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
                pattern: r#"(?i)(password|secret|token|api_key|access_key|client_secret|auth_token|bearer|credentials|private_key|signing_key)\s*[:=]\s*["']?[^\s"']{8,}"#.into(),
                action: synapseed_core::policy::PolicyAction::Redact,
            },
            DlpRule {
                name: "private_key".into(),
                pattern: r"-----BEGIN (RSA |EC |DSA )?PRIVATE KEY-----".into(),
                action: synapseed_core::policy::PolicyAction::Redact,
            },
            // v4.17.1: URI-embedded credentials (postgres://user:pass@host, mongodb://, redis://, etc.)
            DlpRule {
                name: "uri_credentials".into(),
                pattern: r"(?i)(?:postgres(?:ql)?|mysql|mongodb(?:\+srv)?|redis|amqp|ftp|ssh)://[^:/?#\s]+:[^@/?#\s]+@".into(),
                action: synapseed_core::policy::PolicyAction::Redact,
            },
            // v4.17.1: JWT tokens (eyJ... base64 header)
            DlpRule {
                name: "jwt_token".into(),
                pattern: r"eyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}".into(),
                action: synapseed_core::policy::PolicyAction::Redact,
            },
            // v4.17.1: Slack webhook URLs
            DlpRule {
                name: "slack_webhook".into(),
                pattern: r"https://hooks\.slack\.com/services/T[A-Z0-9]+/B[A-Z0-9]+/[A-Za-z0-9]+".into(),
                action: synapseed_core::policy::PolicyAction::Redact,
            },
        ];

        let mut scanner = Self::from_rules(&default_rules);

        // Default whitelist: suppress known false positives in Rust codebases.
        // "token" in CancellationToken, shutdown_token(), etc. is not a secret.
        let default_whitelist = vec![
            r"(?i)token\s*[:=]\s*[A-Z]\w+".to_string(), // Type assignment (e.g. token: CancellationToken)
            r"(?i)shutdown_token".to_string(),            // Common Rust async pattern
        ];
        scanner.set_whitelist(&default_whitelist);

        scanner
    }

    /// Set whitelist patterns. Findings whose matched text matches any
    /// whitelist pattern are suppressed (treated as false positives).
    pub(crate) fn set_whitelist(&mut self, patterns: &[String]) {
        self.whitelist = patterns
            .iter()
            .filter_map(|p| match Regex::new(p) {
                Ok(re) => Some(re),
                Err(e) => {
                    warn!(pattern = %p, error = %e, "Failed to compile DLP whitelist pattern, skipping");
                    None
                }
            })
            .collect();
    }

    /// Check if a matched text is suppressed by the whitelist.
    fn is_whitelisted(&self, matched_text: &str) -> bool {
        self.whitelist.iter().any(|re| re.is_match(matched_text))
    }

    /// Scan content and return all findings.
    ///
    /// Three-pass approach:
    /// - Pass 0: Decode Base64/Hex-encoded fragments and re-scan decoded text
    /// - Pass 1: Aho-Corasick static terms (GB/s throughput)
    /// - Pass 2: Regex patterns for structured secrets
    pub(crate) fn scan(&self, content: &str) -> Vec<Finding> {
        let mut findings = self.scan_plain(content);

        // Pass 0: Decode Base64/Hex-encoded strings and scan decoded content.
        // This catches secrets that were encoded to bypass plaintext rules.
        for decoded in Self::decode_encoded_fragments(content) {
            for mut f in self.scan_plain(&decoded) {
                f.rule_name = format!("encoded:{}", f.rule_name);
                // Offsets don't map to the original content
                f.start = 0;
                f.end = 0;
                findings.push(f);
            }
        }

        findings
    }

    /// Core plaintext scan: Aho-Corasick + Regex.
    fn scan_plain(&self, content: &str) -> Vec<Finding> {
        let mut findings = Vec::new();

        // Pass 1: Aho-Corasick static terms
        if let Some(ref matcher) = self.static_matcher {
            for mat in matcher.find_iter(content) {
                let matched_text = &content[mat.start()..mat.end()];
                if !self.is_whitelisted(matched_text) {
                    findings.push(Finding {
                        rule_name: format!(
                            "static_term:{}",
                            &self.static_terms[mat.pattern().as_usize()]
                        ),
                        start: mat.start(),
                        end: mat.end(),
                    });
                }
            }
        }

        // Pass 2: Regex patterns
        for pat in &self.regex_patterns {
            for mat in pat.regex.find_iter(content) {
                let matched_text = &content[mat.start()..mat.end()];
                if !self.is_whitelisted(matched_text) {
                    findings.push(Finding {
                        rule_name: pat.name.clone(),
                        start: mat.start(),
                        end: mat.end(),
                    });
                }
            }
        }

        findings
    }

    // ── Base64 / Hex decode helpers (no external deps) ──────────────

    /// Find Base64 and Hex-encoded substrings, decode them, and return
    /// any fragments that look like printable ASCII text.
    fn decode_encoded_fragments(content: &str) -> Vec<String> {
        let mut fragments = Vec::new();

        // Base64: 20+ chars from the Base64 alphabet, optional = padding
        let b64_re = Regex::new(r"[A-Za-z0-9+/]{20,}={0,2}").expect("valid regex");
        for mat in b64_re.find_iter(content) {
            if let Some(decoded) = Self::try_base64_decode(mat.as_str()) {
                if decoded.chars().all(|c| c.is_ascii_graphic() || c.is_ascii_whitespace()) {
                    fragments.push(decoded);
                }
            }
        }

        // Hex: 20+ hex chars (even length only)
        let hex_re = Regex::new(r"\b[0-9a-fA-F]{20,}\b").expect("valid regex");
        for mat in hex_re.find_iter(content) {
            let s = mat.as_str();
            if s.len() % 2 == 0 {
                if let Some(decoded) = Self::try_hex_decode(s) {
                    if decoded.chars().all(|c| c.is_ascii_graphic() || c.is_ascii_whitespace()) {
                        fragments.push(decoded);
                    }
                }
            }
        }

        fragments
    }

    /// Minimal Base64 decode (standard alphabet, with padding).
    fn try_base64_decode(input: &str) -> Option<String> {
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut table = [255u8; 128];
        for (i, &b) in alphabet.iter().enumerate() {
            table[b as usize] = i as u8;
        }

        let clean: Vec<u8> = input.bytes().filter(|&b| b != b'=').collect();
        let mut out = Vec::with_capacity(clean.len() * 3 / 4);

        for chunk in clean.chunks(4) {
            let mut buf = [0u8; 4];
            for (i, &b) in chunk.iter().enumerate() {
                if b >= 128 || table[b as usize] == 255 {
                    return None;
                }
                buf[i] = table[b as usize];
            }
            let n = chunk.len();
            if n >= 2 {
                out.push((buf[0] << 2) | (buf[1] >> 4));
            }
            if n >= 3 {
                out.push((buf[1] << 4) | (buf[2] >> 2));
            }
            if n >= 4 {
                out.push((buf[2] << 6) | buf[3]);
            }
        }

        String::from_utf8(out).ok()
    }

    /// Decode a hex string to UTF-8 text.
    fn try_hex_decode(input: &str) -> Option<String> {
        let bytes: Option<Vec<u8>> = (0..input.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&input[i..i + 2], 16).ok())
            .collect();
        bytes.and_then(|b| String::from_utf8(b).ok())
    }

    /// Scan and redact: replace all findings with [REDACTED].
    pub(crate) fn redact(&self, content: &str) -> (String, Vec<Finding>) {
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
pub(crate) struct Finding {
    pub(crate) rule_name: String,
    pub(crate) start: usize,
    pub(crate) end: usize,
}
