use synapseed_core::error::{Result, SynapseedError};

/// Supported languages for AST parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Python,
    JavaScript,
}

impl Language {
    /// Detect language from file extension.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "rs" => Some(Self::Rust),
            "py" | "pyi" => Some(Self::Python),
            "js" | "jsx" | "mjs" => Some(Self::JavaScript),
            _ => None,
        }
    }

    /// Get the tree-sitter language for this variant.
    pub fn ts_language(&self) -> Result<tree_sitter::Language> {
        let lang = match self {
            Self::Rust => tree_sitter_rust::LANGUAGE,
            Self::Python => tree_sitter_python::LANGUAGE,
            Self::JavaScript => tree_sitter_javascript::LANGUAGE,
        };
        Ok(tree_sitter::Language::new(lang))
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::JavaScript => "javascript",
        }
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

impl std::str::FromStr for Language {
    type Err = SynapseedError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "rust" | "rs" => Ok(Self::Rust),
            "python" | "py" => Ok(Self::Python),
            "javascript" | "js" => Ok(Self::JavaScript),
            other => Err(SynapseedError::Internal(format!(
                "Unsupported language: {other}"
            ))),
        }
    }
}
