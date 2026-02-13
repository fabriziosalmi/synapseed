use serde::{Deserialize, Serialize};

/// Action to take when a policy rule matches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyAction {
    Allow,
    Deny,
    Redact,
    Audit,
}

/// A single DLP rule: pattern + what to do when matched.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlpRule {
    pub name: String,
    pub pattern: String,
    pub action: PolicyAction,
}

/// A command execution policy rule for the SSH sentinel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRule {
    pub pattern: String,
    pub action: PolicyAction,
    pub description: Option<String>,
}

/// The full security policy loaded from configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicy {
    #[serde(default)]
    pub dlp_rules: Vec<DlpRule>,
    #[serde(default)]
    pub command_rules: Vec<CommandRule>,
    #[serde(default = "default_true")]
    pub fail_closed: bool,
    /// Regex patterns that suppress false-positive DLP findings.
    #[serde(default)]
    pub dlp_whitelist: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            dlp_rules: Vec::new(),
            command_rules: Vec::new(),
            fail_closed: true,
            dlp_whitelist: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_policy_default_fail_closed() {
        let policy = SecurityPolicy::default();
        assert!(policy.fail_closed);
        assert!(policy.dlp_rules.is_empty());
        assert!(policy.command_rules.is_empty());
        assert!(policy.dlp_whitelist.is_empty());
    }

    #[test]
    fn security_policy_serde_roundtrip() {
        let policy = SecurityPolicy {
            dlp_rules: vec![DlpRule {
                name: "aws_key".into(),
                pattern: "AKIA[0-9A-Z]{16}".into(),
                action: PolicyAction::Deny,
            }],
            command_rules: vec![CommandRule {
                pattern: "rm -rf /".into(),
                action: PolicyAction::Deny,
                description: Some("Block recursive delete of root".into()),
            }],
            fail_closed: false,
            dlp_whitelist: vec!["CancellationToken".into()],
        };
        let json = serde_json::to_string(&policy).unwrap();
        let back: SecurityPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(back.fail_closed, false);
        assert_eq!(back.dlp_rules.len(), 1);
        assert_eq!(back.dlp_rules[0].action, PolicyAction::Deny);
        assert_eq!(back.command_rules.len(), 1);
        assert_eq!(back.dlp_whitelist, vec!["CancellationToken"]);
    }

    #[test]
    fn policy_action_serde_all_variants() {
        for action in [
            PolicyAction::Allow,
            PolicyAction::Deny,
            PolicyAction::Redact,
            PolicyAction::Audit,
        ] {
            let json = serde_json::to_string(&action).unwrap();
            let back: PolicyAction = serde_json::from_str(&json).unwrap();
            assert_eq!(action, back);
        }
    }

    #[test]
    fn security_policy_deserialize_missing_defaults() {
        // fail_closed defaults to true, lists default to empty
        let json = r#"{}"#;
        let policy: SecurityPolicy = serde_json::from_str(json).unwrap();
        assert!(policy.fail_closed);
        assert!(policy.dlp_rules.is_empty());
    }
}
