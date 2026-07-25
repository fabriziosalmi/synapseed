use std::process::Output;
use std::time::Duration;

use synapseed_core::error::{Result, SynapseedError};
use synapseed_core::policy::PolicyAction;
use tracing::{debug, info, warn};

use crate::sentinel::Sentinel;

/// Sandboxed command executor.
///
/// Every command is validated by the Sentinel before execution.
/// The executor captures stdout/stderr and enforces timeouts.
pub struct Executor {
    sentinel: Sentinel,
    timeout_secs: u64,
}

impl Executor {
    pub fn new(sentinel: Sentinel) -> Self {
        Self {
            sentinel,
            timeout_secs: 30,
        }
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Execute a command after policy validation.
    pub async fn execute(&self, command: &str) -> Result<ExecutionResult> {
        // Gate: Sentinel must approve
        let action = self.sentinel.evaluate(command)?;

        match action {
            PolicyAction::Allow | PolicyAction::Audit => {}
            PolicyAction::Deny => {
                return Err(SynapseedError::PolicyDenied {
                    command: command.to_string(),
                });
            }
            PolicyAction::Redact => {
                return Ok(ExecutionResult {
                    stdout: "[REDACTED]".into(),
                    stderr: String::new(),
                    exit_code: 0,
                });
            }
        }

        debug!(
            command = command,
            timeout_secs = self.timeout_secs,
            "Executing sandboxed command"
        );

        let child_future = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .output();

        let output = match tokio::time::timeout(
            Duration::from_secs(self.timeout_secs),
            child_future,
        )
        .await
        {
            Ok(res) => res.map_err(SynapseedError::Io)?,
            Err(_) => {
                warn!(
                    command = command,
                    timeout_secs = self.timeout_secs,
                    "Command timed out"
                );
                return Err(SynapseedError::Internal(format!(
                    "Command timed out after {}s: {}",
                    self.timeout_secs, command
                )));
            }
        };

        let result = ExecutionResult::from_output(output);

        info!(
            command = command,
            exit_code = result.exit_code,
            "Command executed"
        );

        Ok(result)
    }
}

/// The result of a sandboxed command execution.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl ExecutionResult {
    fn from_output(output: Output) -> Self {
        let exit_code = output.status.code().unwrap_or_else(|| {
            // On Unix, process terminated by signal; capture signal number if available
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                output.status.signal().unwrap_or(-1)
            }
            // On non-Unix platforms, fall back to -1
            #[cfg(not(unix))]
            {
                -1
            }
        });

        Self {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code,
        }
    }
}
