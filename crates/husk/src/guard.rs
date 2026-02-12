use synapseed_core::error::{Result, SynapseedError};
use synapseed_core::policy::SecurityPolicy;
use tracing::warn;

use crate::scanner::DlpScanner;

/// The Security Guard — the central DLP enforcement point.
///
/// All content leaving the process must pass through the guard.
/// If `fail_closed` is true (default), any scanner error blocks the content.
pub struct SecurityGuard {
    scanner: DlpScanner,
    fail_closed: bool,
}

impl SecurityGuard {
    /// Create a guard from a security policy.
    pub fn from_policy(policy: &SecurityPolicy) -> Self {
        let scanner = if policy.dlp_rules.is_empty() {
            DlpScanner::with_defaults()
        } else {
            DlpScanner::from_rules(&policy.dlp_rules)
        };

        Self {
            scanner,
            fail_closed: policy.fail_closed,
        }
    }

    /// Create a guard with default detection rules.
    pub fn with_defaults() -> Self {
        Self {
            scanner: DlpScanner::with_defaults(),
            fail_closed: true,
        }
    }

    /// Sanitize content: scan and redact any sensitive data.
    ///
    /// If `fail_closed` is true and findings exist, returns an error
    /// instead of silently redacting. Use `redact()` for force-redaction.
    pub fn sanitize(&self, content: &str) -> Result<String> {
        let (redacted, findings) = self.scanner.redact(content);

        if !findings.is_empty() {
            let rules: Vec<_> = findings.iter().map(|f| f.rule_name.as_str()).collect();

            if self.fail_closed {
                return Err(SynapseedError::SecurityViolation(format!(
                    "DLP: {} finding(s) blocked (fail-closed): {}",
                    findings.len(),
                    rules.join(", ")
                )));
            }

            warn!(
                findings = findings.len(),
                rules = ?rules,
                "DLP: Sensitive content redacted"
            );
        }

        Ok(redacted)
    }

    /// Force-redact content regardless of fail_closed policy.
    pub fn redact(&self, content: &str) -> String {
        let (redacted, findings) = self.scanner.redact(content);

        if !findings.is_empty() {
            warn!(
                findings = findings.len(),
                "DLP: Sensitive content force-redacted"
            );
        }

        redacted
    }

    /// Check content without modifying it. Returns Err if violations found.
    pub fn check(&self, content: &str) -> Result<()> {
        let findings = self.scanner.scan(content);

        if findings.is_empty() {
            Ok(())
        } else {
            let rules: Vec<_> = findings.iter().map(|f| f.rule_name.as_str()).collect();
            Err(SynapseedError::SecurityViolation(format!(
                "DLP violation: {} finding(s) matching rules: {}",
                findings.len(),
                rules.join(", ")
            )))
        }
    }
}
