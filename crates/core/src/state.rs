use std::path::Path;

use serde::{Deserialize, Serialize};
use tracing::debug;

/// Detected state of the project being analyzed.
///
/// SYNAPSEED uses this to inject the right strategy into the LLM context.
/// A VirginRepo gets scaffold suggestions; a HealthyWorkspace gets
/// architecture maps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectState {
    /// Empty or near-empty repo (only README/LICENSE/gitignore)
    VirginRepo,
    /// Has a build file but incomplete structure
    PartialSetup {
        has_build_file: bool,
        has_src: bool,
        missing: Vec<String>,
    },
    /// Fully functional project
    HealthyWorkspace {
        build_system: BuildSystem,
        file_count: usize,
    },
    /// Unknown project type
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildSystem {
    Cargo,
    Npm,
    Poetry,
    Pip,
    Makefile,
    Mixed,
}

impl ProjectState {
    /// Detect the project state by scanning the root directory.
    pub fn detect(root: &Path) -> Self {
        debug!(path = %root.display(), "Detecting project state");

        let has_cargo = root.join("Cargo.toml").exists();
        let has_package_json = root.join("package.json").exists();
        let has_pyproject = root.join("pyproject.toml").exists();
        let has_requirements = root.join("requirements.txt").exists();
        let has_makefile = root.join("Makefile").exists();
        let has_src = root.join("src").is_dir();
        let has_crates = root.join("crates").is_dir();

        let has_any_build = has_cargo || has_package_json || has_pyproject || has_requirements;

        // Count meaningful files (not hidden, not build artifacts)
        let meaningful_files = count_meaningful_files(root);

        // Virgin repo: very few files, no build system
        if !has_any_build && meaningful_files <= 5 {
            debug!("Detected: VirginRepo ({meaningful_files} files, no build system)");
            return Self::VirginRepo;
        }

        // Determine build system
        let build_system = if has_cargo && has_package_json {
            BuildSystem::Mixed
        } else if has_cargo {
            BuildSystem::Cargo
        } else if has_package_json {
            BuildSystem::Npm
        } else if has_pyproject {
            BuildSystem::Poetry
        } else if has_requirements {
            BuildSystem::Pip
        } else if has_makefile {
            BuildSystem::Makefile
        } else {
            return Self::Unknown;
        };

        // Check for partial setup
        let mut missing = Vec::new();
        match build_system {
            BuildSystem::Cargo => {
                if !has_src && !has_crates {
                    missing.push("src/ or crates/ directory".into());
                }
            }
            BuildSystem::Npm => {
                if !has_src && !root.join("lib").is_dir() {
                    missing.push("src/ or lib/ directory".into());
                }
            }
            _ => {}
        }

        if !missing.is_empty() {
            debug!(missing = ?missing, "Detected: PartialSetup");
            return Self::PartialSetup {
                has_build_file: has_any_build,
                has_src,
                missing,
            };
        }

        debug!(
            build_system = ?build_system,
            files = meaningful_files,
            "Detected: HealthyWorkspace"
        );
        Self::HealthyWorkspace {
            build_system,
            file_count: meaningful_files,
        }
    }

    /// Generate a diagnostic string for LLM context injection.
    pub fn diagnostic(&self) -> String {
        match self {
            Self::VirginRepo => "STATUS: UNINITIALIZED (Virgin Repository)\n\
                 DETECTED: No build system found.\n\
                 RECOMMENDED: Bootstrap project structure using `scaffold` command."
                .into(),
            Self::PartialSetup {
                has_build_file,
                has_src,
                missing,
            } => {
                format!(
                    "STATUS: PARTIAL SETUP\n\
                     BUILD FILE: {has_build_file}\n\
                     SRC DIR: {has_src}\n\
                     MISSING: {}\n\
                     RECOMMENDED: Complete project structure.",
                    missing.join(", ")
                )
            }
            Self::HealthyWorkspace {
                build_system,
                file_count,
            } => {
                format!(
                    "STATUS: HEALTHY WORKSPACE\n\
                     BUILD SYSTEM: {build_system:?}\n\
                     FILES: {file_count}\n\
                     READY: Full analysis available."
                )
            }
            Self::Unknown => "STATUS: UNKNOWN PROJECT TYPE\n\
                 RECOMMENDED: Manual configuration via synapseed.toml."
                .into(),
        }
    }
}

fn count_meaningful_files(root: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };

    entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            !name.starts_with('.') && name != "target" && name != "node_modules"
        })
        .count()
}
