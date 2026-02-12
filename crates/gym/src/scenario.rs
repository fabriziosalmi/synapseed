use serde::{Deserialize, Serialize};

/// A training scenario submitted to the Gym for evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    /// The Rust source code to evaluate (injected as `src/lib.rs`).
    pub source_code: String,

    /// Optional test code (injected as `tests/eval.rs`).
    /// If empty, only compilation is verified.
    #[serde(default)]
    pub test_code: String,

    /// Extra Cargo dependencies for the sandbox project.
    /// Each entry is `"crate_name" = "version"` (e.g., `"serde" = "1"`).
    #[serde(default)]
    pub dependencies: Vec<Dependency>,

    /// Maximum time (seconds) for the entire evaluation.
    /// Default: 60.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

/// A Cargo dependency to add to the sandbox project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub version: String,
    /// Optional features to enable.
    #[serde(default)]
    pub features: Vec<String>,
}

fn default_timeout() -> u64 {
    60
}

impl Scenario {
    /// Create a minimal scenario with just source code.
    pub fn new(source_code: impl Into<String>) -> Self {
        Self {
            source_code: source_code.into(),
            test_code: String::new(),
            dependencies: Vec::new(),
            timeout_secs: default_timeout(),
        }
    }

    /// Add test code to the scenario.
    pub fn with_tests(mut self, test_code: impl Into<String>) -> Self {
        self.test_code = test_code.into();
        self
    }

    /// Add a dependency.
    pub fn with_dependency(mut self, name: impl Into<String>, version: impl Into<String>) -> Self {
        self.dependencies.push(Dependency {
            name: name.into(),
            version: version.into(),
            features: Vec::new(),
        });
        self
    }

    /// Set the timeout.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }
}
