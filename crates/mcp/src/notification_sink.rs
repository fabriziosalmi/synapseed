//! NotificationSink — Shared stdout writer for server-initiated MCP notifications.
//!
//! The MCP server normally writes to stdout only in response to client requests.
//! The `NotificationSink` allows background tasks (FileWatcher, RepairOrchestrator)
//! to emit unsolicited JSON-RPC notifications to the client (e.g., "auto-fix proposed").
//!
//! Thread-safe: cloneable sender backed by an mpsc channel drained by a single
//! writer task that serialises all output to stdout.

use serde::Serialize;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tracing::{debug, warn};

/// A JSON-RPC 2.0 notification (no `id` field).
#[derive(Debug, Clone, Serialize)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: String,
    pub params: serde_json::Value,
}

impl Notification {
    /// Create a custom notification for auto-fix proposals.
    pub fn auto_fix_proposed(
        proposal_id: &str,
        file_path: &str,
        error_code: &str,
        preview: &str,
    ) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            method: "notifications/message".into(),
            params: serde_json::json!({
                "level": "info",
                "logger": "repair-orchestrator",
                "data": {
                    "type": "auto_fix_proposed",
                    "proposal_id": proposal_id,
                    "file_path": file_path,
                    "error_code": error_code,
                    "preview": preview,
                }
            }),
        }
    }

    /// Create a notification for auto-fix application result.
    pub fn auto_fix_applied(proposal_id: &str, file_path: &str, success: bool) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            method: "notifications/message".into(),
            params: serde_json::json!({
                "level": if success { "info" } else { "warning" },
                "logger": "repair-orchestrator",
                "data": {
                    "type": "auto_fix_applied",
                    "proposal_id": proposal_id,
                    "file_path": file_path,
                    "success": success,
                }
            }),
        }
    }
}

/// Cloneable sender handle — register as a `SynapseContext` extension.
#[derive(Clone)]
pub struct NotificationSink {
    tx: mpsc::Sender<Notification>,
}

impl NotificationSink {
    /// Send a notification to the client (non-blocking, drops if buffer full).
    pub fn send(&self, notif: Notification) {
        if self.tx.try_send(notif).is_err() {
            warn!("NotificationSink: channel full or closed, dropping notification");
        }
    }
}

/// Spawn the sink writer task and return the cloneable sender handle.
///
/// The writer task reads from the mpsc channel and writes serialised
/// JSON-RPC notifications to stdout, one per line.
pub fn spawn_notification_sink() -> NotificationSink {
    let (tx, mut rx) = mpsc::channel::<Notification>(64);

    tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(notif) = rx.recv().await {
            match serde_json::to_string(&notif) {
                Ok(json) => {
                    // Serialise writes: JSON line + newline + flush
                    if let Err(e) = stdout.write_all(json.as_bytes()).await {
                        warn!(error = %e, "NotificationSink: stdout write failed");
                        break;
                    }
                    let _ = stdout.write_all(b"\n").await;
                    let _ = stdout.flush().await;
                    debug!(method = %notif.method, "NotificationSink: sent notification");
                }
                Err(e) => {
                    warn!(error = %e, "NotificationSink: serialization failed");
                }
            }
        }
        debug!("NotificationSink: writer task exiting");
    });

    NotificationSink { tx }
}
