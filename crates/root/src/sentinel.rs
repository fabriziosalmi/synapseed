use regex::Regex;
use synapseed_core::error::{Result, SynapseedError};
use synapseed_core::policy::{CommandRule, PolicyAction, SecurityPolicy};
use tracing::{info, warn};

/// The Sentinel — policy-driven command gatekeeper.
///
/// Evaluates commands against a list of allow/deny rules before
/// they reach the OS. Denied commands never execute.
pub struct Sentinel {
    rules: Vec<CompiledRule>,
    fail_closed: bool,
}

struct CompiledRule {
    regex: Regex,
    action: PolicyAction,
    description: Option<String>,
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
                    pattern: r"^chmod\s+777".into(),
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
                CommandRule {
                    pattern: r"^(ls|cat|echo|pwd|whoami|git\s+status|git\s+log|git\s+diff|cargo\s+check|cargo\s+test|cargo\s+build)".into(),
                    action: PolicyAction::Allow,
                    description: Some("Common safe commands".into()),
                },
            ],
            fail_closed: true,
        };

        Self::from_policy(&policy)
    }

    /// Evaluate a command. Returns Ok(PolicyAction) if allowed, Err if denied.
    pub fn evaluate(&self, command: &str) -> Result<PolicyAction> {
        let trimmed = command.trim();

        for rule in &self.rules {
            if rule.regex.is_match(trimmed) {
                match rule.action {
                    PolicyAction::Deny => {
                        warn!(
                            command = trimmed,
                            description = rule.description.as_deref().unwrap_or(""),
                            "Sentinel DENIED command"
                        );
                        return Err(SynapseedError::PolicyDenied {
                            command: trimmed.to_string(),
                        });
                    }
                    PolicyAction::Allow => {
                        info!(command = trimmed, "Sentinel ALLOWED command");
                        return Ok(PolicyAction::Allow);
                    }
                    PolicyAction::Audit => {
                        warn!(
                            command = trimmed,
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
                command = trimmed,
                "Sentinel: no matching rule, DENYING (fail-closed)"
            );
            Err(SynapseedError::PolicyDenied {
                command: trimmed.to_string(),
            })
        } else {
            info!(
                command = trimmed,
                "Sentinel: no matching rule, allowing (fail-open)"
            );
            Ok(PolicyAction::Allow)
        }
    }
}
