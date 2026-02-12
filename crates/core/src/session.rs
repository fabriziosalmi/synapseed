use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// Persistent session state saved to `.synapseed/session.json`.
///
/// Enables cross-session continuity: "Welcome Back" messages,
/// resumption of context, and usage metrics across restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub session_id: String,
    pub started_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    pub files_indexed: usize,
    pub tools_invoked: usize,
    pub project_root: String,
}

impl SessionState {
    /// Create a fresh session state for a new session.
    pub fn new(project_root: &Path) -> Self {
        let now = Utc::now();
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            started_at: now,
            last_active: now,
            files_indexed: 0,
            tools_invoked: 0,
            project_root: project_root.display().to_string(),
        }
    }

    /// Load previous session state from `.synapseed/session.json`.
    /// Returns `None` if no session file exists or is malformed.
    pub fn load(project_root: &Path) -> Option<Self> {
        let path = project_root.join(".synapseed").join("session.json");
        let content = std::fs::read_to_string(&path).ok()?;
        match serde_json::from_str(&content) {
            Ok(session) => {
                debug!(path = %path.display(), "Loaded previous session state");
                Some(session)
            }
            Err(e) => {
                warn!(path = %path.display(), error = %e, "Malformed session file, ignoring");
                None
            }
        }
    }

    /// Save session state to `.synapseed/session.json`.
    pub fn save(&self, project_root: &Path) {
        let dir = project_root.join(".synapseed");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            warn!(error = %e, "Cannot create .synapseed directory");
            return;
        }
        let path = dir.join("session.json");
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    warn!(path = %path.display(), error = %e, "Failed to save session state");
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to serialize session state");
            }
        }
    }

    /// Returns true if the last session was active within the last 24 hours.
    pub fn is_recent(&self) -> bool {
        let elapsed = Utc::now() - self.last_active;
        elapsed.num_hours() < 24
    }

    /// Human-readable description of how long ago the session was active.
    pub fn time_ago(&self) -> String {
        let elapsed = Utc::now() - self.last_active;
        if elapsed.num_minutes() < 1 {
            "moments ago".into()
        } else if elapsed.num_minutes() < 60 {
            format!("{} minutes ago", elapsed.num_minutes())
        } else if elapsed.num_hours() < 24 {
            format!("{} hours ago", elapsed.num_hours())
        } else {
            format!("{} days ago", elapsed.num_days())
        }
    }
}
