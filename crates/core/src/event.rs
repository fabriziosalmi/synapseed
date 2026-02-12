use serde::{Deserialize, Serialize};

use crate::state::ProjectState;

/// Domain events that flow through the plugin system.
///
/// Each plugin receives events and can react accordingly.
/// Events are the communication backbone between modules.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SynapseEvent {
    /// System is starting up
    SystemInit {
        project_root: String,
        state: ProjectState,
    },
    /// A file was modified (or created)
    FileChanged { path: String, kind: FileChangeKind },
    /// A symbol was resolved by the cortex
    SymbolResolved {
        name: String,
        file: String,
        line: usize,
    },
    /// DLP found sensitive content
    SecurityAlert {
        rule: String,
        severity: Severity,
        context: String,
    },
    /// A command was evaluated by the sentinel
    CommandEvaluated { command: String, allowed: bool },
    /// User requested a scaffold/bootstrap
    ScaffoldRequested { template: String },
    /// Git state changed (new commit, branch switch, etc.)
    GitStateChanged {
        head: String,
        branch: Option<String>,
    },
    /// Compiler diagnostics updated for a project path
    DiagnosticUpdated {
        /// Number of errors
        errors: usize,
        /// Number of warnings
        warnings: usize,
    },
    /// Telemetry spans received from OTLP
    TelemetryUpdate {
        /// Number of spans in this batch
        spans_received: usize,
        /// File path of the hottest span (if any)
        hotspot_file: Option<String>,
        /// Duration of the hottest span in ms (if any)
        hotspot_duration_ms: Option<f64>,
    },
    /// Code graph indexing completed in the background.
    /// Fired by CortexPlugin after async indexing finishes.
    IndexingComplete,
    /// System is shutting down
    SystemShutdown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeKind {
    Created,
    Modified,
    Deleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}
