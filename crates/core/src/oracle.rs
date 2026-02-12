//! Consistency Oracle — cross-references project artifacts for drift.
//!
//! Detects inconsistencies between:
//! - README documented features vs actual crate directories
//! - Cargo.toml workspace members vs filesystem
//! - Feature claims vs actual implementations

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Full consistency check report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsistencyReport {
    /// Total checks performed.
    pub total_checks: usize,
    /// Detected inconsistencies.
    pub inconsistencies: Vec<Inconsistency>,
    /// Consistency score: 1.0 = fully consistent, 0.0 = all checks failed.
    pub score: f64,
}

/// A single detected inconsistency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inconsistency {
    /// Category: "workspace", "readme", "features", "docs".
    pub category: String,
    /// Severity: "warning" or "error".
    pub severity: String,
    /// Human-readable description.
    pub description: String,
    /// Suggested fix.
    pub suggestion: String,
}

/// Run all consistency checks against the project root.
pub fn check_consistency(project_root: &Path) -> ConsistencyReport {
    let mut inconsistencies = Vec::new();
    let mut total_checks = 0;

    // Check 1: Workspace members vs filesystem
    inconsistencies.extend(check_workspace_members(project_root, &mut total_checks));

    // Check 2: README feature claims vs crate directories
    inconsistencies.extend(check_readme_features(project_root, &mut total_checks));

    // Check 3: Documentation index vs actual doc files
    inconsistencies.extend(check_docs_index(project_root, &mut total_checks));

    // Check 4: Cargo.toml descriptions vs README mentions
    inconsistencies.extend(check_crate_descriptions(project_root, &mut total_checks));

    let score = if total_checks > 0 {
        1.0 - (inconsistencies.len() as f64 / total_checks as f64).min(1.0)
    } else {
        1.0
    };

    ConsistencyReport {
        total_checks,
        inconsistencies,
        score,
    }
}

/// Check that all workspace members listed in root Cargo.toml exist on disk.
fn check_workspace_members(root: &Path, total: &mut usize) -> Vec<Inconsistency> {
    let cargo_path = root.join("Cargo.toml");
    let content = match std::fs::read_to_string(&cargo_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut issues = Vec::new();

    // Simple TOML parsing for workspace members.
    // Look for `members = [...]` block.
    let members = extract_workspace_members(&content);

    for member in &members {
        *total += 1;
        let member_path = root.join(member);
        if !member_path.join("Cargo.toml").exists() {
            issues.push(Inconsistency {
                category: "workspace".to_string(),
                severity: "error".to_string(),
                description: format!(
                    "Workspace member `{member}` listed in Cargo.toml but directory or Cargo.toml missing"
                ),
                suggestion: format!(
                    "Remove `{member}` from workspace members or create the crate directory."
                ),
            });
        }
    }

    // Reverse check: find crate directories that aren't listed as members.
    for dir_name in &["crates", "bin"] {
        let dir = root.join(dir_name);
        if !dir.exists() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.join("Cargo.toml").exists() {
                    *total += 1;
                    let relative = format!("{}/{}", dir_name, entry.file_name().to_string_lossy());
                    let is_member = members.iter().any(|m| m == &relative || m.contains(&relative));
                    if !is_member {
                        issues.push(Inconsistency {
                            category: "workspace".to_string(),
                            severity: "warning".to_string(),
                            description: format!(
                                "Crate at `{relative}` has Cargo.toml but is not listed in workspace members"
                            ),
                            suggestion: format!(
                                "Add `\"{relative}\"` to workspace.members in root Cargo.toml."
                            ),
                        });
                    }
                }
            }
        }
    }

    issues
}

/// Check README feature claims against actual crate directories.
fn check_readme_features(root: &Path, total: &mut usize) -> Vec<Inconsistency> {
    let readme_path = root.join("README.md");
    let content = match std::fs::read_to_string(&readme_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut issues = Vec::new();

    // Extract crate names from crates/ directory.
    let crates_dir = root.join("crates");
    let existing_crates: Vec<String> = if crates_dir.exists() {
        std::fs::read_dir(&crates_dir)
            .ok()
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| e.path().join("Cargo.toml").exists())
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    // Check each crate is mentioned in README.
    for crate_name in &existing_crates {
        *total += 1;
        let lower = content.to_lowercase();
        let patterns = [
            crate_name.to_lowercase(),
            crate_name.replace('-', "_").to_lowercase(),
        ];
        let mentioned = patterns.iter().any(|p| lower.contains(p));
        if !mentioned {
            issues.push(Inconsistency {
                category: "readme".to_string(),
                severity: "warning".to_string(),
                description: format!(
                    "Crate `{crate_name}` exists in crates/ but is not mentioned in README.md"
                ),
                suggestion: format!(
                    "Add a section in README.md describing the `{crate_name}` crate."
                ),
            });
        }
    }

    issues
}

/// Check that documentation index links to existing files.
fn check_docs_index(root: &Path, total: &mut usize) -> Vec<Inconsistency> {
    let index_path = root.join("docs/index.md");
    let content = match std::fs::read_to_string(&index_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut issues = Vec::new();

    // Extract markdown links: [text](path)
    for line in content.lines() {
        for link in extract_md_links(line) {
            if link.starts_with("http") || link.starts_with('#') {
                continue;
            }
            *total += 1;
            let target = root.join("docs").join(&link);
            if !target.exists() {
                issues.push(Inconsistency {
                    category: "docs".to_string(),
                    severity: "error".to_string(),
                    description: format!("docs/index.md links to `{link}` but file does not exist"),
                    suggestion: format!("Create `docs/{link}` or remove the broken link."),
                });
            }
        }
    }

    issues
}

/// Check that each crate with a description in Cargo.toml is findable.
fn check_crate_descriptions(root: &Path, total: &mut usize) -> Vec<Inconsistency> {
    let crates_dir = root.join("crates");
    if !crates_dir.exists() {
        return Vec::new();
    }

    let mut issues = Vec::new();

    for entry in std::fs::read_dir(&crates_dir).ok().into_iter().flatten().flatten() {
        let cargo_path = entry.path().join("Cargo.toml");
        if !cargo_path.exists() {
            continue;
        }
        *total += 1;

        let content = match std::fs::read_to_string(&cargo_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Check for missing description.
        let has_description = content.lines().any(|l| {
            let l = l.trim();
            l.starts_with("description") && l.contains('=')
        });
        if !has_description {
            let name = entry.file_name().to_string_lossy().to_string();
            issues.push(Inconsistency {
                category: "features".to_string(),
                severity: "warning".to_string(),
                description: format!("Crate `{name}` Cargo.toml has no `description` field"),
                suggestion: format!(
                    "Add a `description = \"...\"` to crates/{name}/Cargo.toml."
                ),
            });
        }
    }

    issues
}

// ── Helpers ────────────────────────────────────────────────────────

/// Extract workspace member paths from Cargo.toml content.
fn extract_workspace_members(content: &str) -> Vec<String> {
    let mut members = Vec::new();
    let mut in_members = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("members") && trimmed.contains('=') {
            in_members = true;
            // Handle inline: members = ["a", "b"]
            if let Some(bracket) = trimmed.find('[') {
                let rest = &trimmed[bracket..];
                members.extend(extract_quoted_strings(rest));
                if rest.contains(']') {
                    in_members = false;
                }
            }
            continue;
        }

        if in_members {
            if trimmed.contains(']') {
                members.extend(extract_quoted_strings(trimmed));
                in_members = false;
            } else {
                members.extend(extract_quoted_strings(trimmed));
            }
        }
    }

    members
}

/// Extract quoted strings from a line: `"foo/bar", "baz"` → `["foo/bar", "baz"]`
fn extract_quoted_strings(line: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut in_quote = false;
    let mut current = String::new();

    for ch in line.chars() {
        if ch == '"' {
            if in_quote {
                if !current.is_empty() {
                    result.push(current.clone());
                }
                current.clear();
            }
            in_quote = !in_quote;
        } else if in_quote {
            current.push(ch);
        }
    }

    result
}

/// Extract markdown link targets from a line: `[text](target)` → `["target"]`
fn extract_md_links(line: &str) -> Vec<String> {
    let mut links = Vec::new();
    let mut rest = line;

    while let Some(open) = rest.find("](") {
        let after = &rest[open + 2..];
        if let Some(close) = after.find(')') {
            links.push(after[..close].to_string());
            rest = &after[close + 1..];
        } else {
            break;
        }
    }

    links
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_workspace_members() {
        let toml = r#"
[workspace]
members = [
    "bin/synapseed",
    "crates/core",
    "crates/mcp",
]
"#;
        let members = extract_workspace_members(toml);
        assert_eq!(members, vec!["bin/synapseed", "crates/core", "crates/mcp"]);
    }

    #[test]
    fn test_extract_md_links() {
        let line = "- [Overview](architecture/overview.md) and [Guide](guide/intro.md)";
        let links = extract_md_links(line);
        assert_eq!(links, vec!["architecture/overview.md", "guide/intro.md"]);
    }

    #[test]
    fn test_check_consistency_on_synapseed() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();

        let report = check_consistency(root);
        assert!(report.total_checks > 0, "Expected some consistency checks");
        assert!(report.score >= 0.0 && report.score <= 1.0);
    }
}
