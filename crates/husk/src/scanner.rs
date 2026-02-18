use aho_corasick::AhoCorasick;
use regex::Regex;
use synapseed_core::policy::DlpRule;
use tracing::{debug, warn};

/// A compiled DLP scanner that detects secrets and sensitive patterns.
///
/// Uses a two-pass approach:
/// 1. Aho-Corasick automaton for static terms (GB/s throughput)
/// 2. Regex patterns for structured secrets (API keys, tokens)
///
/// An optional whitelist suppresses false-positive findings: if the matched
/// text contains any compiled whitelist pattern, the finding is dropped.
///
/// D36: A Shannon entropy gate filters out low-entropy matches that are
/// unlikely to be real secrets (e.g., Base64-encoded SVG icons).
pub(crate) struct DlpScanner {
    /// Fast multi-pattern matcher for static terms
    static_matcher: Option<AhoCorasick>,
    static_terms: Vec<String>,
    /// Compiled regex patterns for structured secrets
    regex_patterns: Vec<CompiledPattern>,
    /// Compiled whitelist patterns (suppress findings whose matched text matches)
    whitelist: Vec<Regex>,
    /// D36: Minimum Shannon entropy (bits per char) for a regex match to be
    /// considered a secret. Low-entropy strings are likely false positives.
    min_entropy: f64,
}

/// D36: Default minimum Shannon entropy threshold.
/// Real secrets (API keys, tokens) typically have entropy ≥ 3.5 bits/char.
/// Structured text and code identifiers are usually below 3.0.
const DEFAULT_MIN_ENTROPY: f64 = 3.0;

struct CompiledPattern {
    name: String,
    regex: Regex,
    /// Skip the Shannon entropy gate for this pattern.
    /// Contextual patterns (e.g., `generic_secret`) use the variable name
    /// as signal — low-entropy values like `secret123` are still real secrets.
    skip_entropy: bool,
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
                        skip_entropy: false,
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
            min_entropy: DEFAULT_MIN_ENTROPY,
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
            // v4.17.2 (W9): URI-embedded credentials — generic protocol (catches postgres, mongodb, redis, amqp, etc.)
            DlpRule {
                name: "uri_credentials".into(),
                pattern: r"(?i)[a-z][a-z0-9+.-]*://[^:/?#\s]+:[^@/?#\s]+@".into(),
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
            // v4.24.0 (D87): PII patterns — email addresses
            DlpRule {
                name: "pii_email".into(),
                pattern: r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}".into(),
                action: synapseed_core::policy::PolicyAction::Redact,
            },
            // v4.24.0 (D87): PII patterns — phone numbers (requires + prefix or separator)
            DlpRule {
                name: "pii_phone".into(),
                pattern: r"\+[1-9]\d{0,2}[\s.-]?\(?\d{2,4}\)?[\s.-]?\d{3,4}[\s.-]?\d{3,4}".into(),
                action: synapseed_core::policy::PolicyAction::Redact,
            },
            // v4.24.0 (D87): PII patterns — IPv4 addresses (non-loopback)
            DlpRule {
                name: "pii_ipv4".into(),
                pattern: r"(?:(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\.){3}(?:25[0-5]|2[0-4]\d|[01]?\d\d?)".into(),
                action: synapseed_core::policy::PolicyAction::Redact,
            },
        ];

        let mut scanner = Self::from_rules(&default_rules);

        // generic_secret matches by variable name context (password, secret, etc.),
        // so the entropy of the assigned value is irrelevant — skip the entropy gate.
        for pat in &mut scanner.regex_patterns {
            if pat.name == "generic_secret" {
                pat.skip_entropy = true;
            }
        }

        // Default whitelist: suppress known false positives in Rust codebases.
        // "token" in CancellationToken, shutdown_token(), etc. is not a secret.
        let default_whitelist = vec![
            r"(?i)token\s*[:=]\s*[A-Z]\w+".to_string(), // Type assignment (e.g. token: CancellationToken)
            r"(?i)shutdown_token".to_string(),          // Common Rust async pattern
            r"127\.0\.0\.1".to_string(),                // D87: localhost is not PII
            r"0\.0\.0\.0".to_string(),                  // D87: bind-all is not PII
            r"(?i)example\.com".to_string(),            // D87: example domains are not PII
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

    /// D36: Calculate Shannon entropy (bits per character) of a string.
    ///
    /// Real secrets have high randomness (typically ≥ 3.5 bits/char).
    /// Structured text, code identifiers, and Base64-encoded images
    /// usually have lower entropy (< 3.0).
    fn shannon_entropy(s: &str) -> f64 {
        if s.is_empty() {
            return 0.0;
        }
        let mut freq = [0u32; 256];
        for &b in s.as_bytes() {
            freq[b as usize] += 1;
        }
        let len = s.len() as f64;
        let mut entropy = 0.0_f64;
        for &count in &freq {
            if count > 0 {
                let p = count as f64 / len;
                entropy -= p * p.log2();
            }
        }
        entropy
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
        // D36: Entropy gate applied to decoded fragments before re-scanning.
        for decoded in self.decode_encoded_fragments(content) {
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

        // Pass 2: Regex patterns (D36: with Shannon entropy gate)
        self.scan_regex_pass(content, &mut findings);

        // D22: Pass 2b — re-scan merged concatenation blocks to catch secrets
        // split across multiple string literals (e.g., "postgres" + "ql://user:pass@host").
        let merged = Self::merge_concatenation_lines(content);
        if merged != content {
            let mut concat_findings = Vec::new();
            self.scan_regex_pass(&merged, &mut concat_findings);
            for mut f in concat_findings {
                // Deduplicate: skip if same rule already found in Pass 2.
                if findings
                    .iter()
                    .any(|existing| existing.rule_name == f.rule_name)
                {
                    continue;
                }
                // Offsets don't map to the original content.
                f.rule_name = format!("concat:{}", f.rule_name);
                f.start = 0;
                f.end = 0;
                findings.push(f);
            }
        }

        findings
    }

    /// D22: Merge adjacent lines that form string concatenations.
    ///
    /// Strips string delimiters (`"`, `'`, `` ` ``), concatenation operators (`+`),
    /// commas, and whitespace to reconstruct the effective string value.
    /// This catches patterns like:
    /// ```text
    /// let db = "postgres"
    ///         + "ql://"
    ///         + "admin:secret@host";
    /// ```
    fn merge_concatenation_lines(content: &str) -> String {
        // Quick check: if no string concatenation operator, skip entirely.
        if !content.contains("\" +") && !content.contains("' +") && !content.contains("` +") {
            return content.to_string();
        }

        let concat_strip = Regex::new(r#"["'`]\s*\+\s*["'`]"#).expect("valid regex");

        // Replace all `" + "` / `' + '` / `` ` + ` `` patterns with empty string,
        // effectively joining the adjacent string literals.
        concat_strip.replace_all(content, "").to_string()
    }

    /// Run regex patterns against content, appending findings.
    fn scan_regex_pass(&self, content: &str, findings: &mut Vec<Finding>) {
        for pat in &self.regex_patterns {
            for mat in pat.regex.find_iter(content) {
                let matched_text = &content[mat.start()..mat.end()];
                if !self.is_whitelisted(matched_text) {
                    // D36: Skip low-entropy matches to reduce false positives.
                    // Extract the "value" portion after the `=` or `:` separator
                    // for entropy calculation; if no separator, use the whole match.
                    let value_part = matched_text
                        .find(['=', ':'])
                        .map(|pos| {
                            matched_text[pos + 1..]
                                .trim()
                                .trim_matches(|c: char| c == '"' || c == '\'')
                        })
                        .filter(|v| v.len() >= 8)
                        .unwrap_or(matched_text);
                    // D36: Skip entropy gate for URI credential patterns
                    // (they contain "://") — the structural chars dilute entropy.
                    // Also skip for contextual patterns (skip_entropy) where the
                    // variable name provides sufficient signal.
                    let is_uri = matched_text.contains("://");
                    let entropy = Self::shannon_entropy(value_part);
                    if !is_uri
                        && !pat.skip_entropy
                        && entropy < self.min_entropy
                        && value_part.len() >= 8
                    {
                        debug!(
                            rule = %pat.name,
                            entropy = format!("{:.2}", entropy),
                            threshold = format!("{:.2}", self.min_entropy),
                            "DLP: skipping low-entropy match (D36)"
                        );
                        continue;
                    }
                    findings.push(Finding {
                        rule_name: pat.name.clone(),
                        start: mat.start(),
                        end: mat.end(),
                    });
                }
            }
        }
    }

    // ── Base64 / Hex decode helpers (no external deps) ──────────────

    /// Find Base64 and Hex-encoded substrings, decode them, and return
    /// any fragments that look like printable ASCII text.
    ///
    /// D36: Applies Shannon entropy gate — decoded blobs with entropy below
    /// `min_entropy` are dropped (likely structured data, not secrets).
    fn decode_encoded_fragments(&self, content: &str) -> Vec<String> {
        let mut fragments = Vec::new();

        // Base64: 20+ chars from the Base64 alphabet, optional = padding
        let b64_re = Regex::new(r"[A-Za-z0-9+/]{20,}={0,2}").expect("valid regex");
        for mat in b64_re.find_iter(content) {
            if let Some(decoded) = Self::try_base64_decode(mat.as_str()) {
                if decoded
                    .chars()
                    .all(|c| c.is_ascii_graphic() || c.is_ascii_whitespace())
                {
                    let entropy = Self::shannon_entropy(&decoded);
                    if entropy < self.min_entropy {
                        debug!(
                            entropy = format!("{:.2}", entropy),
                            threshold = format!("{:.2}", self.min_entropy),
                            len = decoded.len(),
                            "DLP: skipping low-entropy Base64 fragment (D36)"
                        );
                        continue;
                    }
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
                    if decoded
                        .chars()
                        .all(|c| c.is_ascii_graphic() || c.is_ascii_whitespace())
                    {
                        let entropy = Self::shannon_entropy(&decoded);
                        if entropy < self.min_entropy {
                            debug!(
                                entropy = format!("{:.2}", entropy),
                                threshold = format!("{:.2}", self.min_entropy),
                                len = decoded.len(),
                                "DLP: skipping low-entropy Hex fragment (D36)"
                            );
                            continue;
                        }
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
