use std::path::Path;
use std::sync::Mutex;

use git2::{BlameOptions, Repository};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use synapseed_core::error::{Result, SynapseedError};

use crate::truncate_oid;

/// A historian that reads Git history for code archaeology.
///
/// Repository is wrapped in Mutex to satisfy Send + Sync
/// (git2::Repository contains raw pointers internally).
pub struct Historian {
    repo: Mutex<Repository>,
}

/// Information about a Git commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    pub id: String,
    pub message: String,
    pub author: String,
    pub timestamp: String,
    pub files_changed: usize,
}

/// Blame information for a specific line range.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlameInfo {
    pub line: usize,
    pub commit_id: String,
    pub author: String,
    pub message: String,
    pub timestamp: String,
    /// D30: Co-authors extracted from `Co-authored-by:` trailers.
    /// Enables AI attribution tracking for quickfix-generated changes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub co_authors: Vec<String>,
}

/// Summary of the repository state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSummary {
    pub head_commit: String,
    pub branch: Option<String>,
    pub total_commits: usize,
    pub recent_commits: Vec<CommitInfo>,
    pub is_dirty: bool,
}

impl Historian {
    /// Open a repository at the given path.
    #[instrument(skip_all, fields(path = %root.display()))]
    pub fn open(root: &Path) -> Result<Self> {
        let repo = Repository::discover(root)
            .map_err(|e| SynapseedError::Internal(format!("Failed to open git repo: {e}")))?;
        Ok(Self {
            repo: Mutex::new(repo),
        })
    }

    pub(crate) fn with_repo<T, F: FnOnce(&Repository) -> Result<T>>(&self, f: F) -> Result<T> {
        let repo = self
            .repo
            .lock()
            .map_err(|e| SynapseedError::Internal(format!("Repo mutex poisoned: {e}")))?;
        f(&repo)
    }

    /// Get the current HEAD commit ID.
    pub fn head_id(&self) -> Result<String> {
        self.with_repo(|repo| {
            let head = repo
                .head()
                .map_err(|e| SynapseedError::Internal(format!("Failed to read HEAD: {e}")))?;
            let oid = head
                .target()
                .ok_or_else(|| SynapseedError::Internal("HEAD has no target".into()))?;
            Ok(oid.to_string())
        })
    }

    /// Get the current branch name.
    pub fn current_branch(&self) -> Option<String> {
        self.with_repo(|repo| {
            let head = repo
                .head()
                .map_err(|e| SynapseedError::Internal(format!("Failed to read HEAD: {e}")))?;
            Ok(head.shorthand().map(String::from))
        })
        .ok()
        .flatten()
    }

    /// Check if the working directory has uncommitted changes.
    pub fn is_dirty(&self) -> bool {
        self.with_repo(|repo| {
            let mut opts = git2::StatusOptions::new();
            opts.include_untracked(true);
            let dirty = repo
                .statuses(Some(&mut opts))
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            Ok(dirty)
        })
        .unwrap_or(false)
    }

    /// Get a summary of the repository state.
    #[instrument(skip(self))]
    pub fn summary(&self, max_recent: usize) -> Result<RepoSummary> {
        let head_id = self.head_id()?;
        let branch = self.current_branch();
        let is_dirty = self.is_dirty();
        let recent_commits = self.recent_commits(max_recent)?;
        let total_commits = self.count_commits()?;

        Ok(RepoSummary {
            head_commit: head_id,
            branch,
            total_commits,
            recent_commits,
            is_dirty,
        })
    }

    /// Get the N most recent commits.
    #[instrument(skip(self))]
    pub fn recent_commits(&self, limit: usize) -> Result<Vec<CommitInfo>> {
        self.with_repo(|repo| {
            let mut revwalk = repo
                .revwalk()
                .map_err(|e| SynapseedError::Internal(format!("Failed to walk commits: {e}")))?;
            // Empty repos have no HEAD — return empty list instead of error
            if revwalk.push_head().is_err() {
                return Ok(Vec::new());
            }

            let mut commits = Vec::new();

            for oid in revwalk.take(limit) {
                let oid =
                    oid.map_err(|e| SynapseedError::Internal(format!("Revwalk error: {e}")))?;
                let commit = repo
                    .find_commit(oid)
                    .map_err(|e| SynapseedError::Internal(format!("Failed to find commit: {e}")))?;

                let author = commit.author();
                let timestamp = chrono::DateTime::from_timestamp(commit.time().seconds(), 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| "unknown".into());

                commits.push(CommitInfo {
                    id: truncate_oid(&oid.to_string()),
                    message: commit
                        .message()
                        .unwrap_or("")
                        .lines()
                        .next()
                        .unwrap_or("")
                        .to_string(),
                    author: author.name().unwrap_or("unknown").to_string(),
                    timestamp,
                    files_changed: 0,
                });
            }

            Ok(commits)
        })
    }

    /// Get blame information for a file at specific line range.
    #[instrument(skip(self))]
    pub fn blame_lines(
        &self,
        file_path: &str,
        start_line: usize,
        end_line: usize,
    ) -> Result<Vec<BlameInfo>> {
        self.with_repo(|repo| {
            let mut opts = BlameOptions::new();
            opts.min_line(start_line);
            opts.max_line(end_line);

            let blame = repo
                .blame_file(Path::new(file_path), Some(&mut opts))
                .map_err(|e| {
                    SynapseedError::Internal(format!("Failed to blame {file_path}: {e}"))
                })?;

            let mut results = Vec::new();

            for hunk in blame.iter() {
                let sig = hunk.final_signature();
                let commit_id = hunk.final_commit_id();

                let commit_obj = repo.find_commit(commit_id).ok();
                let full_message = commit_obj
                    .as_ref()
                    .and_then(|c| c.message().map(String::from))
                    .unwrap_or_default();
                let message = full_message.lines().next().unwrap_or("").to_string();

                // D30: Extract Co-authored-by trailers for AI attribution tracking.
                let co_authors: Vec<String> = full_message
                    .lines()
                    .filter_map(|line| {
                        line.trim()
                            .strip_prefix("Co-authored-by:")
                            .or_else(|| line.trim().strip_prefix("Co-Authored-By:"))
                            .map(|rest| rest.trim().to_string())
                    })
                    .collect();

                let timestamp = chrono::DateTime::from_timestamp(sig.when().seconds(), 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| "unknown".into());

                results.push(BlameInfo {
                    line: hunk.final_start_line(),
                    commit_id: truncate_oid(&commit_id.to_string()),
                    author: sig.name().unwrap_or("unknown").to_string(),
                    message,
                    timestamp,
                    co_authors,
                });
            }

            Ok(results)
        })
    }

    fn count_commits(&self) -> Result<usize> {
        self.with_repo(|repo| {
            let mut revwalk = repo
                .revwalk()
                .map_err(|e| SynapseedError::Internal(format!("Failed to walk commits: {e}")))?;
            if revwalk.push_head().is_err() {
                return Ok(0);
            }
            Ok(revwalk.take(10_000).count())
        })
    }
}
