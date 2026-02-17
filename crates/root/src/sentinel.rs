use regex::Regex;
use synapseed_core::error::{Result, SynapseedError};
use synapseed_core::policy::{CommandRule, PolicyAction, SecurityPolicy};
use tracing::{info, warn};

/// The Sentinel — policy-driven command gatekeeper.
///
/// Evaluates commands against a list of allow/deny rules before
/// they reach the OS. Denied commands never execute.
///
/// **v4.29.0 hardening:** Shell chaining detection (`;`, `|`, `&&`, `||`,
/// newlines), null byte rejection, command substitution and obfuscation
/// deny rules.
pub struct Sentinel {
    rules: Vec<CompiledRule>,
    fail_closed: bool,
}

struct CompiledRule {
    regex: Regex,
    action: PolicyAction,
    description: Option<String>,
}

/// Split a command string on shell chaining operators for independent evaluation.
/// Handles: `;` `||` `&&` `|` and newlines.
fn split_shell_segments(command: &str) -> Vec<String> {
    let re = Regex::new(r"\s*(?:;|\|\||&&|\|)\s*|\n").expect("split regex");
    re.split(command)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

impl Sentinel {
    /// Build a sentinel from a security policy.
    pub fn from_policy(policy: &SecurityPolicy) -> Result<Self> {
        let mut rules = Vec::new();

        for rule in &policy.command_rules {
            let regex = Regex::new(&rule.pattern).map_err(|e| {
                SynapseedError::Internal(format!(
                    "Invalid command rule regex '{}': {e}",
                    rule.pattern
                ))
            })?;

            rules.push(CompiledRule {
                regex,
                action: rule.action.clone(),
                description: rule.description.clone(),
            });
        }

        Ok(Self {
            rules,
            fail_closed: policy.fail_closed,
        })
    }

    /// Create a sentinel with sensible default rules.
    pub fn with_defaults() -> Result<Self> {
        let policy = SecurityPolicy {
            dlp_rules: Vec::new(),
            command_rules: vec![
                // ── Deny rules (evaluated first) ────────────────────
                CommandRule {
                    pattern: r"^rm\s+(-[rRf]+\s+)?/".into(),
                    action: PolicyAction::Deny,
                    description: Some("Block recursive delete from root".into()),
                },
                CommandRule {
                    pattern: r"^(mkfs|dd|fdisk|parted)".into(),
                    action: PolicyAction::Deny,
                    description: Some("Block disk operations".into()),
                },
                CommandRule {
                    pattern: r"^chmod\s+(0?777|a\+[rwx]{3}|a=rwx)".into(),
                    action: PolicyAction::Deny,
                    description: Some("Block world-writable permissions".into()),
                },
                CommandRule {
                    pattern: r">\s*/dev/sd[a-z]".into(),
                    action: PolicyAction::Deny,
                    description: Some("Block raw device writes".into()),
                },
                CommandRule {
                    pattern: r"^sudo\b".into(),
                    action: PolicyAction::Deny,
                    description: Some("Block privilege escalation".into()),
                },
                CommandRule {
                    pattern: r"\beval\b".into(),
                    action: PolicyAction::Deny,
                    description: Some("Block shell eval".into()),
                },
                CommandRule {
                    pattern: r"curl\b.*\|\s*(ba)?sh".into(),
                    action: PolicyAction::Deny,
                    description: Some("Block piped curl-to-shell execution".into()),
                },
                CommandRule {
                    pattern: r"LD_PRELOAD\s*=".into(),
                    action: PolicyAction::Deny,
                    description: Some("Block LD_PRELOAD injection".into()),
                },
                // v4.29.0: Command substitution
                CommandRule {
                    pattern: r"\$\(".into(),
                    action: PolicyAction::Deny,
                    description: Some("Block command substitution $()".into()),
                },
                CommandRule {
                    pattern: r"`".into(),
                    action: PolicyAction::Deny,
                    description: Some("Block backtick command substitution".into()),
                },
                // v4.29.0: Obfuscation vectors
                CommandRule {
                    pattern: r"base64\s+(-d|--decode)".into(),
                    action: PolicyAction::Deny,
                    description: Some("Block base64 decode (obfuscation vector)".into()),
                },
                CommandRule {
                    pattern: r"^(python[23]?|ruby|perl|node)\s+-[ce]\b".into(),
                    action: PolicyAction::Deny,
                    description: Some("Block interpreter inline execution".into()),
                },
                CommandRule {
                    pattern: r"^nohup\b".into(),
                    action: PolicyAction::Deny,
                    description: Some("Block nohup (session escape)".into()),
                },
                // ── Allow rules ─────────────────────────────────────
                CommandRule {
                    pattern: r"^(ls|cat|echo|pwd|whoami|git\s+status|git\s+log|git\s+diff|cargo\s+check|cargo\s+test|cargo\s+build)".into(),
                    action: PolicyAction::Allow,
                    description: Some("Common safe commands".into()),
                },
            ],
            fail_closed: true,
            dlp_whitelist: Vec::new(),
        };

        Self::from_policy(&policy)
    }

    /// Evaluate a command. Returns Ok(PolicyAction) if allowed, Err if denied.
    ///
    /// Multi-segment commands (chained via `;`, `|`, `&&`, `||`, or newlines)
    /// are split and each segment is evaluated independently. If ANY segment
    /// is denied, the entire command is denied.
    pub fn evaluate(&self, command: &str) -> Result<PolicyAction> {
        let trimmed = command.trim();

        // Reject null bytes (C-string truncation attacks)
        if trimmed.bytes().any(|b| b == 0) {
            warn!("Sentinel DENIED: null byte in command");
            return Err(SynapseedError::PolicyDenied {
                command: trimmed.to_string(),
            });
        }

        // Split on shell chaining operators
        let segments = split_shell_segments(trimmed);
        if segments.len() <= 1 {
            return self.evaluate_single(trimmed);
        }

        // Multi-segment: evaluate each independently.
        // ANY deny → deny all. ALL must be explicitly allowed/audit.
        let mut overall = PolicyAction::Allow;
        for seg in &segments {
            match self.evaluate_single(seg) {
                Ok(PolicyAction::Allow) => {}
                Ok(PolicyAction::Audit) => {
                    overall = PolicyAction::Audit;
                }
                Ok(PolicyAction::Redact) => {
                    overall = PolicyAction::Redact;
                }
                Ok(PolicyAction::Deny) | Err(_) => {
                    warn!(
                        command = trimmed,
                        segment = seg.as_str(),
                        "Sentinel DENIED chained command"
                    );
                    return Err(SynapseedError::PolicyDenied {
                        command: trimmed.to_string(),
                    });
                }
            }
        }

        if matches!(overall, PolicyAction::Allow) {
            info!(command = trimmed, "Sentinel ALLOWED chained command");
        }
        Ok(overall)
    }

    /// Evaluate a single command segment against the rule set.
    fn evaluate_single(&self, command: &str) -> Result<PolicyAction> {
        for rule in &self.rules {
            if rule.regex.is_match(command) {
                match rule.action {
                    PolicyAction::Deny => {
                        warn!(
                            command = command,
                            description = rule.description.as_deref().unwrap_or(""),
                            "Sentinel DENIED command"
                        );
                        return Err(SynapseedError::PolicyDenied {
                            command: command.to_string(),
                        });
                    }
                    PolicyAction::Allow => {
                        info!(command = command, "Sentinel ALLOWED command");
                        return Ok(PolicyAction::Allow);
                    }
                    PolicyAction::Audit => {
                        warn!(
                            command = command,
                            "Sentinel AUDIT: command permitted but logged"
                        );
                        return Ok(PolicyAction::Audit);
                    }
                    PolicyAction::Redact => {
                        return Ok(PolicyAction::Redact);
                    }
                }
            }
        }

        // No rule matched
        if self.fail_closed {
            warn!(
                command = command,
                "Sentinel: no matching rule, DENYING (fail-closed)"
            );
            Err(SynapseedError::PolicyDenied {
                command: command.to_string(),
            })
        } else {
            info!(
                command = command,
                "Sentinel: no matching rule, allowing (fail-open)"
            );
            Ok(PolicyAction::Allow)
        }
    }
}
