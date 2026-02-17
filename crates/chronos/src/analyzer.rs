//! History Analyzer — churn, co-change, semantic commit patterns.
//!
//! Extends `Historian` with deep historical analysis capabilities.
//! Answers questions like "Why is this function so complicated?" by
//! analyzing commit frequency, co-changes, and semantic patterns.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use git2::{BlameOptions, DiffOptions, Sort};
use serde::Serialize;

use synapseed_core::error::{Result, SynapseedError};

use crate::historian::Historian;
use crate::truncate_oid;

/// Semantic tag for a commit based on its message content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitTag {
    Fix,
    Revert,
    Refactor,
    Feature,
    Security,
    Performance,
    Test,
    Docs,
    Other,
}

/// A commit enriched with semantic tags.
#[derive(Debug, Clone, Serialize)]
pub struct AnalyzedCommit {
    pub id: String,
    pub message: String,
    pub author: String,
    pub timestamp: String,
    pub epoch: i64,
    pub tags: Vec<CommitTag>,
}

/// A file that frequently changes alongside the target file.
#[derive(Debug, Clone, Serialize)]
pub struct CoChange {
    pub file_path: String,
    pub co_change_count: usize,
    pub co_change_ratio: f64,
}

/// Summary of semantic patterns across all analyzed commits.
#[derive(Debug, Clone, Serialize)]
pub struct SemanticSummary {
    pub fix_count: usize,
    pub revert_count: usize,
    pub refactor_count: usize,
    pub feature_count: usize,
    pub security_count: usize,
    pub performance_count: usize,
    pub risk_indicator: String,
}

/// High-level semantic summary of recent commit intent.
#[derive(Debug, Clone, Serialize)]
pub struct IntentSummary {
    /// Human-readable summary (e.g., "5 commits over 3 days: 3 refactoring, 1 fix, 1 feature")
    pub summary: String,
    /// Category → count mapping
    pub categories: HashMap<String, usize>,
    /// Number of commits analyzed
    pub total_commits: usize,
    /// Human-readable time span (e.g., "3 days", "2 weeks")
    pub time_span: Option<String>,
}

/// Full history analysis result for a file (optionally scoped to a line range).
#[derive(Debug, Clone, Serialize)]
pub struct HistoryAnalysis {
    pub file_path: String,
    pub line_range: Option<(usize, usize)>,
    pub total_commits: usize,
    pub hotspot_score: f64,
    pub top_authors: Vec<(String, usize)>,
    pub last_fix_commit: Option<AnalyzedCommit>,
    pub semantic_summary: SemanticSummary,
    pub co_changes: Vec<CoChange>,
    /// Convergence rate: 1.0 = stable, lower = stuck in fix cycles.
    pub convergence_rate: f64,
    /// Rigidity: fix_chain_count / total_commits. High = repeated rework.
    pub rigidity: f64,
    /// Number of fix-chain sequences (consecutive fixes within 48h).
    pub fix_chain_count: usize,
    pub commits: Vec<AnalyzedCommit>,
}

/// Classify a commit message into semantic tags.
fn tag_commit_message(message: &str) -> Vec<CommitTag> {
    let lower = message.to_lowercase();
    let mut tags = Vec::new();

    if lower.contains("fix")
        || lower.contains("bug")
        || lower.contains("patch")
        || lower.contains("hotfix")
        || lower.contains("resolve")
    {
        tags.push(CommitTag::Fix);
    }
    if lower.contains("revert") || lower.contains("rollback") || lower.contains("undo") {
        tags.push(CommitTag::Revert);
    }
    if lower.contains("refactor") || lower.contains("cleanup") || lower.contains("restructure") {
        tags.push(CommitTag::Refactor);
    }
    if lower.contains("feat")
        || lower.contains("add ")
        || lower.contains("new ")
        || lower.contains("implement")
        || lower.contains("introduce")
    {
        tags.push(CommitTag::Feature);
    }
    if lower.contains("security")
        || lower.contains("vuln")
        || lower.contains("cve")
        || lower.contains("xss")
        || lower.contains("injection")
        || lower.contains("leak")
    {
        tags.push(CommitTag::Security);
    }
    if lower.contains("perf")
        || lower.contains("optim")
        || lower.contains("speed")
        || lower.contains("fast")
        || lower.contains("cache")
    {
        tags.push(CommitTag::Performance);
    }
    if lower.contains("test") || lower.contains("spec") || lower.contains("assert") {
        tags.push(CommitTag::Test);
    }
    if lower.contains("doc") || lower.contains("readme") || lower.contains("comment") {
        tags.push(CommitTag::Docs);
    }

    if tags.is_empty() {
        tags.push(CommitTag::Other);
    }

    tags
}

impl Historian {
    /// Analyze the full history of a file, optionally scoped to a line range.
    ///
    /// Returns churn metrics, co-change analysis, semantic commit classification,
    /// and a risk indicator. Scans up to 500 commits for performance.
    pub fn analyze_history(
        &self,
        file_path: &str,
        line_start: Option<usize>,
        line_end: Option<usize>,
    ) -> Result<HistoryAnalysis> {
        self.with_repo(|repo| {
            let mut revwalk = repo
                .revwalk()
                .map_err(|e| SynapseedError::Internal(format!("Failed to walk commits: {e}")))?;
            // Empty repos have no HEAD — return empty analysis
            if revwalk.push_head().is_err() {
                return Ok(HistoryAnalysis {
                    file_path: file_path.to_string(),
                    line_range: line_start.zip(line_end),
                    total_commits: 0,
                    hotspot_score: 0.0,
                    top_authors: Vec::new(),
                    semantic_summary: SemanticSummary {
                        fix_count: 0,
                        revert_count: 0,
                        refactor_count: 0,
                        feature_count: 0,
                        security_count: 0,
                        performance_count: 0,
                        risk_indicator: "none".to_string(),
                    },
                    co_changes: Vec::new(),
                    convergence_rate: 1.0,
                    rigidity: 0.0,
                    fix_chain_count: 0,
                    commits: Vec::new(),
                    last_fix_commit: None,
                });
            }
            revwalk
                .set_sorting(Sort::TIME)
                .map_err(|e| SynapseedError::Internal(format!("Failed to set sort: {e}")))?;

            let mut file_commits: Vec<AnalyzedCommit> = Vec::new();
            let mut co_change_map: HashMap<String, usize> = HashMap::new();
            let mut author_counts: HashMap<String, usize> = HashMap::new();

            let max_commits = 500;
            let deadline = Instant::now() + Duration::from_secs(10);

            for oid in revwalk.take(max_commits) {
                if Instant::now() > deadline {
                    tracing::warn!("analyze: deadline exceeded after {}ms, returning partial results", 10_000);
                    break;
                }
                let oid = match oid {
                    Ok(o) => o,
                    Err(_) => continue,
                };
                let commit = match repo.find_commit(oid) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let tree = match commit.tree() {
                    Ok(t) => t,
                    Err(_) => continue,
                };

                // Diff against parent (empty tree for root commit)
                let parent_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());

                let mut path_opts = DiffOptions::new();
                path_opts.pathspec(file_path);

                let diff = match repo.diff_tree_to_tree(
                    parent_tree.as_ref(),
                    Some(&tree),
                    Some(&mut path_opts),
                ) {
                    Ok(d) => d,
                    Err(_) => continue,
                };

                if diff.deltas().len() == 0 {
                    continue;
                }

                // This commit touches our file
                let author = commit.author();
                let author_name = author.name().unwrap_or("unknown").to_string();
                let message = commit.message().unwrap_or("").to_string();
                let first_line = message.lines().next().unwrap_or("").to_string();
                let epoch = commit.time().seconds();

                let timestamp = chrono::DateTime::from_timestamp(epoch, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| "unknown".into());

                let tags = tag_commit_message(&first_line);
                *author_counts.entry(author_name.clone()).or_insert(0) += 1;

                file_commits.push(AnalyzedCommit {
                    id: truncate_oid(&oid.to_string()),
                    message: first_line,
                    author: author_name,
                    timestamp,
                    epoch,
                    tags,
                });

                // Co-change: find all other files changed in this commit
                let mut full_opts = DiffOptions::new();
                let full_diff = match repo.diff_tree_to_tree(
                    parent_tree.as_ref(),
                    Some(&tree),
                    Some(&mut full_opts),
                ) {
                    Ok(d) => d,
                    Err(_) => continue,
                };

                for delta in full_diff.deltas() {
                    if let Some(path) = delta.new_file().path() {
                        let path_str = path.to_string_lossy().to_string();
                        if path_str != file_path {
                            *co_change_map.entry(path_str).or_insert(0) += 1;
                        }
                    }
                }
            }

            // If line range specified, filter via blame
            let commits = if line_start.is_some() || line_end.is_some() {
                let ls = line_start.unwrap_or(1);
                let le = line_end.unwrap_or(ls + 50);

                let mut blame_opts = BlameOptions::new();
                blame_opts.min_line(ls);
                blame_opts.max_line(le);

                match repo.blame_file(Path::new(file_path), Some(&mut blame_opts)) {
                    Ok(blame) => {
                        let blame_ids: Vec<String> = blame
                            .iter()
                            .map(|h| truncate_oid(&h.final_commit_id().to_string()))
                            .collect();

                        if blame_ids.is_empty() {
                            file_commits
                        } else {
                            file_commits
                                .into_iter()
                                .filter(|c| blame_ids.contains(&c.id))
                                .collect()
                        }
                    }
                    Err(_) => file_commits,
                }
            } else {
                file_commits
            };

            let total = commits.len();

            // Hotspot score: modifications per month, scaled 0-100,
            // with exponential temporal decay: score × e^(−λ × days_since_newest).
            let hotspot_score = if total > 1 {
                let newest_epoch = commits.first().map(|c| c.epoch).unwrap_or(0);
                let oldest_epoch = commits.last().map(|c| c.epoch).unwrap_or(0);
                let span_days = ((newest_epoch - oldest_epoch) as f64 / 86400.0).max(1.0);
                let mods_per_month = (total as f64 / span_days) * 30.0;
                let raw_score = (mods_per_month * 20.0).min(100.0);

                // Temporal decay: older hotspots cool down.
                let now_epoch = chrono::Utc::now().timestamp();
                let days_since_newest = ((now_epoch - newest_epoch) as f64 / 86400.0).max(0.0);
                let lambda = 0.05; // half-life ≈ 14 days
                raw_score * (-lambda * days_since_newest).exp()
            } else {
                total as f64 // 0 or 1
            };

            // Top authors by commit count
            let mut top_authors: Vec<(String, usize)> = author_counts.into_iter().collect();
            top_authors.sort_by(|a, b| b.1.cmp(&a.1));
            top_authors.truncate(5);

            // Semantic summary
            let fix_count = commits
                .iter()
                .filter(|c| c.tags.contains(&CommitTag::Fix))
                .count();
            let revert_count = commits
                .iter()
                .filter(|c| c.tags.contains(&CommitTag::Revert))
                .count();
            let refactor_count = commits
                .iter()
                .filter(|c| c.tags.contains(&CommitTag::Refactor))
                .count();
            let feature_count = commits
                .iter()
                .filter(|c| c.tags.contains(&CommitTag::Feature))
                .count();
            let security_count = commits
                .iter()
                .filter(|c| c.tags.contains(&CommitTag::Security))
                .count();
            let performance_count = commits
                .iter()
                .filter(|c| c.tags.contains(&CommitTag::Performance))
                .count();

            let risk_indicator = if security_count > 0 || revert_count > 2 {
                "HIGH — security changes or frequent reverts detected"
            } else if fix_count > 0 && fix_count > total / 2 {
                "MEDIUM — majority of changes are bug fixes"
            } else if hotspot_score > 50.0 {
                "MEDIUM — high modification frequency (hotspot)"
            } else {
                "LOW — stable code area"
            };

            let last_fix_commit = commits
                .iter()
                .find(|c| c.tags.contains(&CommitTag::Fix))
                .cloned();

            // Co-changes: filter to files changed together more than once
            let mut co_changes: Vec<CoChange> = co_change_map
                .into_iter()
                .filter(|(_, count)| *count > 1)
                .map(|(path, count)| CoChange {
                    file_path: path,
                    co_change_count: count,
                    co_change_ratio: count as f64 / total.max(1) as f64,
                })
                .collect();
            co_changes.sort_by(|a, b| b.co_change_count.cmp(&a.co_change_count));
            co_changes.truncate(10);

            let line_range = match (line_start, line_end) {
                (Some(s), Some(e)) => Some((s, e)),
                (Some(s), None) => Some((s, s + 50)),
                _ => None,
            };

            // Fix-chain detection: consecutive fix commits within a 48h window.
            let mut fix_chain_count = 0usize;
            let mut in_chain = false;
            for pair in commits.windows(2) {
                let newer = &pair[0];
                let older = &pair[1];
                let both_fix = newer.tags.contains(&CommitTag::Fix)
                    && older.tags.contains(&CommitTag::Fix);
                let within_48h = newer.epoch.saturating_sub(older.epoch) < 48 * 3600;
                if both_fix && within_48h {
                    if !in_chain {
                        fix_chain_count += 1;
                        in_chain = true;
                    }
                } else {
                    in_chain = false;
                }
            }

            let convergence_rate = if total > 1 {
                (1.0 - (fix_chain_count as f64 / total as f64)).max(0.0)
            } else {
                1.0
            };

            let rigidity = if total > 0 {
                fix_chain_count as f64 / total as f64
            } else {
                0.0
            };

            Ok(HistoryAnalysis {
                file_path: file_path.to_string(),
                line_range,
                total_commits: total,
                hotspot_score,
                top_authors,
                last_fix_commit,
                semantic_summary: SemanticSummary {
                    fix_count,
                    revert_count,
                    refactor_count,
                    feature_count,
                    security_count,
                    performance_count,
                    risk_indicator: risk_indicator.to_string(),
                },
                co_changes,
                convergence_rate,
                rigidity,
                fix_chain_count,
                commits: commits.into_iter().take(20).collect(),
            })
        })
    }

    /// Summarize the intent of recent commits semantically.
    ///
    /// Groups commits by semantic tag and builds a natural-language summary.
    pub fn summarize_intent(&self, limit: usize) -> Result<IntentSummary> {
        self.with_repo(|repo| {
            let mut revwalk = repo
                .revwalk()
                .map_err(|e| SynapseedError::Internal(format!("Failed to walk commits: {e}")))?;
            // Empty repos have no HEAD — return empty summary
            if revwalk.push_head().is_err() {
                return Ok(IntentSummary {
                    summary: "No commits found.".to_string(),
                    categories: HashMap::new(),
                    total_commits: 0,
                    time_span: None,
                });
            }
            revwalk
                .set_sorting(git2::Sort::TIME)
                .map_err(|e| SynapseedError::Internal(format!("Failed to set sort: {e}")))?;

            let mut categories: HashMap<String, usize> = HashMap::new();
            let mut scopes: HashMap<String, Vec<String>> = HashMap::new();
            let mut newest_epoch: Option<i64> = None;
            let mut oldest_epoch: Option<i64> = None;
            let mut total = 0usize;
            let deadline = Instant::now() + Duration::from_secs(10);

            for oid in revwalk.take(limit) {
                if Instant::now() > deadline {
                    tracing::warn!("intent_summary: deadline exceeded, returning partial results");
                    break;
                }
                let oid = match oid {
                    Ok(o) => o,
                    Err(_) => continue,
                };
                let commit = match repo.find_commit(oid) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                let message = commit.message().unwrap_or("").to_string();
                let first_line = message.lines().next().unwrap_or("");
                let epoch = commit.time().seconds();

                if newest_epoch.is_none() {
                    newest_epoch = Some(epoch);
                }
                oldest_epoch = Some(epoch);
                total += 1;

                let tags = tag_commit_message(first_line);
                let scope = extract_scope(first_line);

                for tag in &tags {
                    let label = format!("{tag:?}").to_lowercase();
                    *categories.entry(label.clone()).or_insert(0) += 1;
                    if let Some(ref s) = scope {
                        scopes.entry(label).or_default().push(s.clone());
                    }
                }
            }

            // Compute time span
            let time_span = match (newest_epoch, oldest_epoch) {
                (Some(newest), Some(oldest)) if newest > oldest => {
                    let days = (newest - oldest) / 86400;
                    Some(if days == 0 {
                        "today".to_string()
                    } else if days == 1 {
                        "1 day".to_string()
                    } else if days < 7 {
                        format!("{days} days")
                    } else if days < 30 {
                        let weeks = days / 7;
                        format!("{weeks} week{}", if weeks == 1 { "" } else { "s" })
                    } else {
                        let months = days / 30;
                        format!("{months} month{}", if months == 1 { "" } else { "s" })
                    })
                }
                _ => None,
            };

            // Build natural-language summary
            let mut sorted_cats: Vec<(String, usize)> =
                categories.iter().map(|(k, v)| (k.clone(), *v)).collect();
            sorted_cats.sort_by(|a, b| b.1.cmp(&a.1));

            let parts: Vec<String> = sorted_cats
                .iter()
                .map(|(cat, count)| {
                    let scope_list = scopes.get(cat).map(|s| {
                        let mut deduped: Vec<String> = s.clone();
                        deduped.sort();
                        deduped.dedup();
                        deduped.truncate(3);
                        deduped
                    });
                    match scope_list {
                        Some(ref sl) if !sl.is_empty() => {
                            format!("{count} {cat} ({})", sl.join(", "))
                        }
                        _ => format!("{count} {cat}"),
                    }
                })
                .collect();

            let span_str = time_span.as_deref().unwrap_or("recent history");
            let summary = format!("{total} commits over {span_str}: {}", parts.join(", "));

            Ok(IntentSummary {
                summary,
                categories,
                total_commits: total,
                time_span,
            })
        })
    }
}

/// Extract scope from conventional commit format: `type(scope): message` -> Some("scope")
fn extract_scope(message: &str) -> Option<String> {
    let paren_start = message.find('(')?;
    let paren_end = message.find(')')?;
    if paren_end > paren_start + 1 && message[paren_end..].starts_with("):") {
        Some(message[paren_start + 1..paren_end].to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tag_fix_commit() {
        let tags = tag_commit_message("fix: resolve race condition in auth");
        assert!(tags.contains(&CommitTag::Fix));
    }

    #[test]
    fn test_tag_feature_commit() {
        let tags = tag_commit_message("feat: add OAuth2 support");
        assert!(tags.contains(&CommitTag::Feature));
    }

    #[test]
    fn test_tag_security_commit() {
        let tags = tag_commit_message("security: patch CVE-2024-1234 XSS vulnerability");
        assert!(tags.contains(&CommitTag::Security));
    }

    #[test]
    fn test_tag_revert_commit() {
        let tags = tag_commit_message("revert: rollback broken migration");
        assert!(tags.contains(&CommitTag::Revert));
    }

    #[test]
    fn test_tag_multiple_tags() {
        let tags = tag_commit_message("fix: refactor auth to patch security leak");
        assert!(tags.contains(&CommitTag::Fix));
        assert!(tags.contains(&CommitTag::Security));
    }

    #[test]
    fn test_tag_unknown_commit() {
        let tags = tag_commit_message("initial commit");
        assert!(tags.contains(&CommitTag::Other));
    }
}
