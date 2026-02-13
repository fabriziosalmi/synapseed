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

    /// DLP whitelist: regex patterns that suppress false-positive findings.
    /// If a finding's matched text contains any whitelist pattern, it is ignored.
    /// Example: `"CancellationToken"` suppresses `generic_secret` hits on Rust concurrency types.
    #[serde(default)]
    pub dlp_whitelist: Vec<String>,

    /// Search index configuration
    #[serde(default)]
    pub search: SearchConfig,

    /// Visualizer port override (default: 3000)
    #[serde(default)]
    pub visualizer_port: Option<u16>,

    /// Architect configuration (layers, thresholds)
    #[serde(default)]
    pub architect: ArchitectConfig,

    /// HCI (Human-Computer Interaction) configuration.
    #[serde(default)]
    pub hci: HciConfig,

    /// Code security pattern scanning configuration.
    #[serde(default)]
    pub security_patterns: SecurityPatternsConfig,

    /// Context configuration for the Whisperer (symbol pruning, token budget).
    #[serde(default)]
    pub context: ContextConfig,
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

    /// Temporal decay λ for search ranking (default: 0.01).
    /// Higher values penalize older results more aggressively.
    /// Score formula: bm25 × (0.7 + 0.3 × e^(−λ × age_days)).
    #[serde(default)]
    pub temporal_decay_lambda: Option<f64>,
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
    /// Topological density above this triggers a warning (default: 0.5).
    #[serde(default)]
    pub density_high_threshold: Option<f64>,
    /// Topological density below this (with enough modules) triggers a warning (default: 0.02).
    #[serde(default)]
    pub density_low_threshold: Option<f64>,
    /// Minimum module count before low-density check kicks in (default: 10).
    #[serde(default)]
    pub density_low_min_modules: Option<usize>,
}

/// HCI (Human-Computer Interaction) configuration.
/// Controls perceptual quality features: background indexing, adaptive linting,
/// session persistence, and other UX behaviors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HciConfig {
    /// Enable background (non-blocking) code indexing at startup.
    #[serde(default = "default_true")]
    pub background_indexing: bool,
    /// Enable automatic port retry for the Visualizer dashboard.
    #[serde(default = "default_true")]
    pub port_retry: bool,
    /// Enable adaptive linting (debounce escalation during rapid edits).
    #[serde(default = "default_true")]
    pub adaptive_linting: bool,
    /// Enable mentor mode (response depth adapts to query complexity).
    #[serde(default = "default_true")]
    pub mentor_mode: bool,
    /// Enable session persistence (cross-session continuity).
    #[serde(default = "default_true")]
    pub session_persistence: bool,
    /// Max files to index (memory ceiling). None = unlimited (default: 10000).
    #[serde(default)]
    pub memory_ceiling_files: Option<usize>,
    /// Model cognitive profile override: "atomic", "molecular", or "galactic".
    /// When set, overrides auto-detected tier from MCP client fingerprinting.
    #[serde(default)]
    pub model_profile: Option<String>,
}

impl Default for HciConfig {
    fn default() -> Self {
        Self {
            background_indexing: true,
            port_retry: true,
            adaptive_linting: true,
            mentor_mode: true,
            session_persistence: true,
            memory_ceiling_files: None,
            model_profile: None,
        }
    }
}

/// Code security pattern scanning configuration.
/// Controls which vulnerability categories are checked by the `scan_security` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPatternsConfig {
    /// Enable code pattern scanning (default: true).
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Active categories: "sql_injection", "xss", "command_injection", "path_traversal".
    /// Empty = all categories active (default).
    #[serde(default)]
    pub categories: Vec<String>,
}

impl Default for SecurityPatternsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            categories: Vec::new(), // empty = all active
        }
    }
}

/// Context configuration for the Whisperer.
/// Controls how many symbols and how much source code is included in responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    /// Maximum symbols to include in context (default: 5).
    /// Lower values reduce noise for ultra-small models (<3B).
    #[serde(default = "default_max_symbols")]
    pub max_symbols: usize,

    /// Minimum confidence score (0.0–1.0) for semantic search results.
    /// Results below this threshold are discarded to reduce noise.
    /// Default: 0.15.
    #[serde(default = "default_min_confidence")]
    pub min_confidence: f32,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_symbols: default_max_symbols(),
            min_confidence: default_min_confidence(),
        }
    }
}

fn default_min_confidence() -> f32 {
    0.15
}

fn default_max_symbols() -> usize {
    5
}

fn default_true() -> bool {
    true
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
            dlp_whitelist: Vec::new(),
            search: SearchConfig::default(),
            visualizer_port: None,
            architect: ArchitectConfig::default(),
            hci: HciConfig::default(),
            security_patterns: SecurityPatternsConfig::default(),
            context: ContextConfig::default(),
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
        if !other.dlp_whitelist.is_empty() {
            self.dlp_whitelist.extend(other.dlp_whitelist);
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
        // HCI: only override if explicitly set to non-default
        if !other.hci.background_indexing {
            self.hci.background_indexing = false;
        }
        if !other.hci.port_retry {
            self.hci.port_retry = false;
        }
        if !other.hci.adaptive_linting {
            self.hci.adaptive_linting = false;
        }
        if !other.hci.mentor_mode {
            self.hci.mentor_mode = false;
        }
        if !other.hci.session_persistence {
            self.hci.session_persistence = false;
        }
        if other.hci.memory_ceiling_files.is_some() {
            self.hci.memory_ceiling_files = other.hci.memory_ceiling_files;
        }
        if other.hci.model_profile.is_some() {
            self.hci.model_profile = other.hci.model_profile;
        }
        // Security patterns: override if explicitly disabled or categories provided
        if !other.security_patterns.enabled {
            self.security_patterns.enabled = false;
        }
        if !other.security_patterns.categories.is_empty() {
            self.security_patterns.categories = other.security_patterns.categories;
        }
        // Context: override if non-default
        if other.context.max_symbols != default_max_symbols() {
            self.context.max_symbols = other.context.max_symbols;
        }
        if (other.context.min_confidence - default_min_confidence()).abs() > f32::EPSILON {
            self.context.min_confidence = other.context.min_confidence;
        }
    }
}

/// Get the user-level config path: ~/.config/synapseed/dna.yaml
fn user_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("synapseed").join("dna.yaml"))
}
