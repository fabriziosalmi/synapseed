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
        }
    }
}
