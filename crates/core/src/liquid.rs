use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Dynamic configuration system — "The DNA" of SYNAPSEED.
///
/// Configuration is loaded with a cascading priority:
/// 1. Project-level: `.synapseed/dna.yaml` (highest priority)
/// 2. User-level: `~/.config/synapseed/dna.yaml`
/// 3. Embedded defaults (lowest priority)
///
/// Each level overrides the one below it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDna {
    /// Workspace layout strategy
    #[serde(default = "default_strategy")]
    pub workspace_strategy: String,

    /// Preferred libraries (e.g., async: "tokio")
    #[serde(default)]
    pub preferred_libs: std::collections::HashMap<String, String>,

    /// Naming conventions
    #[serde(default)]
    pub naming: NamingConventions,

    /// Enabled plugins
    #[serde(default = "default_plugins")]
    pub plugins: Vec<String>,

    /// Custom scaffold templates
    #[serde(default)]
    pub templates: std::collections::HashMap<String, String>,

    /// DLP sensitivity level
    #[serde(default = "default_dlp_level")]
    pub dlp_level: DlpLevel,

    /// Custom DLP patterns (merged with defaults)
    #[serde(default)]
    pub dlp_custom_rules: Vec<crate::policy::DlpRule>,

    /// Search index configuration
    #[serde(default)]
    pub search: SearchConfig,

    /// Visualizer port override (default: 3000)
    #[serde(default)]
    pub visualizer_port: Option<u16>,

    /// Architect configuration (layers, thresholds)
    #[serde(default)]
    pub architect: ArchitectConfig,
}

/// Search index configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchConfig {
    /// Persist Tantivy index to `.synapseed/index/` on disk.
    /// Default: false (RAM-only, fast startup).
    #[serde(default)]
    pub persistence: bool,

    /// Enable vector embedding similarity search.
    /// Downloads ~22MB model on first use to `.synapseed/models/`.
    #[serde(default)]
    pub embeddings: bool,
}

/// Architect module configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArchitectConfig {
    /// Layer definitions for violation detection.
    #[serde(default)]
    pub layers: Vec<ArchitectLayer>,
    /// Max public symbols before flagging as god object (default: 50).
    #[serde(default)]
    pub god_object_max_symbols: Option<usize>,
    /// Max approximate lines before flagging as god object (default: 1000).
    #[serde(default)]
    pub god_object_max_lines: Option<usize>,
    /// Min fan-in to combine with size for god object detection (default: 5).
    #[serde(default)]
    pub god_object_min_fan_in: Option<usize>,
}

/// A named architectural layer with rank and module patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectLayer {
    /// Layer name (e.g., "core", "domain", "api", "ui").
    pub name: String,
    /// Layer rank (0 = bottom). Lower must not import from higher.
    pub rank: u32,
    /// Module name patterns belonging to this layer.
    pub modules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamingConventions {
    #[serde(default = "default_core_name")]
    pub core_crate: String,
    #[serde(default = "default_bin_name")]
    pub bin_name: String,
}

impl Default for NamingConventions {
    fn default() -> Self {
        Self {
            core_crate: default_core_name(),
            bin_name: default_bin_name(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DlpLevel {
    Off,
    Low,
    Standard,
    Strict,
    Paranoid,
}

fn default_strategy() -> String {
    "monorepo".into()
}
fn default_plugins() -> Vec<String> {
    vec![
        "cortex".into(),
        "husk".into(),
        "root".into(),
        "chronos".into(),
    ]
}
fn default_dlp_level() -> DlpLevel {
    DlpLevel::Standard
}
fn default_core_name() -> String {
    "core".into()
}
fn default_bin_name() -> String {
    "synapseed".into()
}

impl Default for ProjectDna {
    fn default() -> Self {
        Self {
            workspace_strategy: default_strategy(),
            preferred_libs: [
                ("async".into(), "tokio".into()),
                ("json".into(), "serde_json".into()),
                ("error".into(), "thiserror".into()),
            ]
            .into(),
            naming: NamingConventions::default(),
            plugins: default_plugins(),
            templates: std::collections::HashMap::new(),
            dlp_level: default_dlp_level(),
            dlp_custom_rules: Vec::new(),
            search: SearchConfig::default(),
            visualizer_port: None,
            architect: ArchitectConfig::default(),
        }
    }
}

impl ProjectDna {
    /// Load DNA with cascading priority:
    /// project-level > user-level > embedded defaults.
    pub fn load(project_root: &Path) -> Self {
        let mut dna = Self::default();

        // Layer 1: User-level config
        if let Some(user_path) = user_config_path() {
            if user_path.exists() {
                match Self::load_from_file(&user_path) {
                    Ok(user_dna) => {
                        info!(path = %user_path.display(), "Loaded user-level DNA");
                        dna.merge(user_dna);
                    }
                    Err(e) => warn!(
                        path = %user_path.display(),
                        error = %e,
                        "Failed to load user DNA"
                    ),
                }
            }
        }

        // Layer 2: Project-level config (highest priority)
        let project_path = project_root.join(".synapseed").join("dna.yaml");
        if project_path.exists() {
            match Self::load_from_file(&project_path) {
                Ok(project_dna) => {
                    info!(path = %project_path.display(), "Loaded project-level DNA");
                    dna.merge(project_dna);
                }
                Err(e) => warn!(
                    path = %project_path.display(),
                    error = %e,
                    "Failed to load project DNA"
                ),
            }
        } else {
            debug!("No project-level DNA found, using defaults");
        }

        dna
    }

    fn load_from_file(path: &Path) -> std::result::Result<Self, String> {
        let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;

        // Support both YAML and TOML based on extension
        match path.extension().and_then(|e| e.to_str()) {
            Some("yaml" | "yml") => serde_yaml::from_str(&content).map_err(|e| e.to_string()),
            Some("toml") => toml::from_str(&content).map_err(|e| e.to_string()),
            _ => serde_yaml::from_str(&content).map_err(|e| e.to_string()),
        }
    }

    /// Merge another DNA into this one (other takes priority).
    fn merge(&mut self, other: Self) {
        if other.workspace_strategy != default_strategy() {
            self.workspace_strategy = other.workspace_strategy;
        }
        for (k, v) in other.preferred_libs {
            self.preferred_libs.insert(k, v);
        }
        if other.naming.core_crate != default_core_name() {
            self.naming.core_crate = other.naming.core_crate;
        }
        if other.naming.bin_name != default_bin_name() {
            self.naming.bin_name = other.naming.bin_name;
        }
        if !other.plugins.is_empty() && other.plugins != default_plugins() {
            self.plugins = other.plugins;
        }
        for (k, v) in other.templates {
            self.templates.insert(k, v);
        }
        if other.dlp_level != default_dlp_level() {
            self.dlp_level = other.dlp_level;
        }
        if !other.dlp_custom_rules.is_empty() {
            self.dlp_custom_rules = other.dlp_custom_rules;
        }
        if other.search.persistence {
            self.search.persistence = true;
        }
        if other.search.embeddings {
            self.search.embeddings = true;
        }
        if other.visualizer_port.is_some() {
            self.visualizer_port = other.visualizer_port;
        }
        if !other.architect.layers.is_empty() {
            self.architect.layers = other.architect.layers;
        }
        if other.architect.god_object_max_symbols.is_some() {
            self.architect.god_object_max_symbols = other.architect.god_object_max_symbols;
        }
        if other.architect.god_object_max_lines.is_some() {
            self.architect.god_object_max_lines = other.architect.god_object_max_lines;
        }
        if other.architect.god_object_min_fan_in.is_some() {
            self.architect.god_object_min_fan_in = other.architect.god_object_min_fan_in;
        }
    }
}

/// Get the user-level config path: ~/.config/synapseed/dna.yaml
fn user_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("synapseed").join("dna.yaml"))
}
