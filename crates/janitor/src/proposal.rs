use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A validated fix proposal that the Janitor wants to apply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    /// Unique proposal identifier.
    pub id: String,
    /// Category of the issue found.
    pub category: ProposalCategory,
    /// The clippy/rustc lint code (e.g. "clippy::manual_map").
    pub lint_code: String,
    /// File path relative to project root.
    pub file_path: String,
    /// Line range in the original file.
    pub line_start: u32,
    pub line_end: u32,
    /// Human-readable description of the issue.
    pub description: String,
    /// Original code snippet that will be replaced.
    pub original_code: String,
    /// Proposed fixed code snippet.
    pub fixed_code: String,
    /// Whether the fix was validated (Gym compile check or MachineApplicable).
    pub validated: bool,
    /// Gym score if validation was run (None if skipped).
    pub gym_score: Option<f64>,
    /// Current status of this proposal.
    pub status: ProposalStatus,
    /// ISO 8601 timestamp when this proposal was created.
    pub created_at: String,
}

/// Category of maintenance issue.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalCategory {
    /// Clippy lint with auto-fix.
    Clippy,
    /// Compiler warning.
    CompilerWarning,
    /// Unused dependency in Cargo.toml.
    UnusedDependency,
}

/// Lifecycle status of a proposal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    /// Waiting for user review.
    Pending,
    /// User applied the fix.
    Applied,
    /// User rejected the fix.
    Rejected,
}

impl Proposal {
    /// Create a new pending proposal.
    pub fn new(
        category: ProposalCategory,
        lint_code: &str,
        file_path: &str,
        line_start: u32,
        line_end: u32,
        description: &str,
        original_code: &str,
        fixed_code: &str,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            category,
            lint_code: lint_code.to_string(),
            file_path: file_path.to_string(),
            line_start,
            line_end,
            description: description.to_string(),
            original_code: original_code.to_string(),
            fixed_code: fixed_code.to_string(),
            validated: false,
            gym_score: None,
            status: ProposalStatus::Pending,
            created_at: Utc::now().to_rfc3339(),
        }
    }
}

/// Thread-safe store for proposals, registered as a context extension.
#[derive(Debug)]
pub struct ProposalStore {
    proposals: DashMap<String, Proposal>,
}

impl ProposalStore {
    pub fn new() -> Self {
        Self {
            proposals: DashMap::new(),
        }
    }

    /// Add a proposal to the store.
    pub fn add(&self, proposal: Proposal) {
        self.proposals.insert(proposal.id.clone(), proposal);
    }

    /// Get a proposal by ID.
    pub fn get(&self, id: &str) -> Option<Proposal> {
        self.proposals.get(id).map(|p| p.clone())
    }

    /// List all pending proposals.
    pub fn pending(&self) -> Vec<Proposal> {
        self.proposals
            .iter()
            .filter(|p| p.status == ProposalStatus::Pending)
            .map(|p| p.clone())
            .collect()
    }

    /// List all proposals regardless of status.
    pub fn all(&self) -> Vec<Proposal> {
        self.proposals.iter().map(|p| p.clone()).collect()
    }

    /// Mark a proposal as applied.
    pub fn mark_applied(&self, id: &str) -> bool {
        if let Some(mut p) = self.proposals.get_mut(id) {
            p.status = ProposalStatus::Applied;
            true
        } else {
            false
        }
    }

    /// Mark a proposal as rejected.
    pub fn mark_rejected(&self, id: &str) -> bool {
        if let Some(mut p) = self.proposals.get_mut(id) {
            p.status = ProposalStatus::Rejected;
            true
        } else {
            false
        }
    }

    /// Clear all proposals (e.g. before a fresh scan).
    pub fn clear(&self) {
        self.proposals.clear();
    }

    /// Number of pending proposals.
    pub fn pending_count(&self) -> usize {
        self.proposals
            .iter()
            .filter(|p| p.status == ProposalStatus::Pending)
            .count()
    }

    /// Total number of proposals.
    pub fn total_count(&self) -> usize {
        self.proposals.len()
    }
}

impl Default for ProposalStore {
    fn default() -> Self {
        Self::new()
    }
}
