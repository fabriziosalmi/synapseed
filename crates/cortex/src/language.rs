use synapseed_core::error::{Result, SynapseedError};

/// Supported languages for AST parsing, plus a fallback for unrecognized extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Language {
    Rust,
    Python,
    JavaScript,
    /// Fallback for files with recognized source extensions but no tree-sitter grammar.
    /// These are still indexed (line count, TODO/FIXME extraction) but without AST parsing.
    Unknown,
}

impl Language {
    /// Detect language from file extension.
    ///
    /// Returns `Some(Language::Unknown)` for recognized source files that lack
    /// a tree-sitter grammar, and `None` for truly non-source files (images, binaries, etc.).
    pub(crate) fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            // Full AST support
            "rs" => Some(Self::Rust),
            "py" | "pyi" => Some(Self::Python),
            "js" | "jsx" | "mjs" => Some(Self::JavaScript),
            // Recognized source — text-only fallback
            "ts" | "tsx" | "mts" | "cts" => Some(Self::Unknown),
            "go" => Some(Self::Unknown),
            "java" | "kt" | "kts" => Some(Self::Unknown),
            "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some(Self::Unknown),
            "cs" => Some(Self::Unknown),
            "rb" | "erb" => Some(Self::Unknown),
            "swift" => Some(Self::Unknown),
            "scala" | "sc" => Some(Self::Unknown),
            "php" => Some(Self::Unknown),
            "lua" => Some(Self::Unknown),
            "zig" => Some(Self::Unknown),
            "nim" => Some(Self::Unknown),
            "ex" | "exs" => Some(Self::Unknown),
            "hs" => Some(Self::Unknown),
            "ml" | "mli" => Some(Self::Unknown),
            "sh" | "bash" | "zsh" | "fish" => Some(Self::Unknown),
            "yaml" | "yml" | "toml" | "json" | "xml" => Some(Self::Unknown),
            "md" | "txt" | "rst" => Some(Self::Unknown),
            "sql" => Some(Self::Unknown),
            "proto" => Some(Self::Unknown),
            "dockerfile" => Some(Self::Unknown),
            _ => None,
        }
    }

    /// Returns true if this language has full tree-sitter AST support.
    pub(crate) fn has_ast_support(&self) -> bool {
        !matches!(self, Self::Unknown)
    }

    /// Get the tree-sitter language for this variant.
    /// Returns an error for `Unknown` since it has no grammar.
    pub(crate) fn ts_language(&self) -> Result<tree_sitter::Language> {
        let lang = match self {
            Self::Rust => tree_sitter_rust::LANGUAGE,
            Self::Python => tree_sitter_python::LANGUAGE,
            Self::JavaScript => tree_sitter_javascript::LANGUAGE,
            Self::Unknown => {
                return Err(SynapseedError::Internal(
                    "No tree-sitter grammar for unknown language".into(),
                ));
            }
        };
        Ok(tree_sitter::Language::new(lang))
    }

    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::JavaScript => "javascript",
            Self::Unknown => "unknown",
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
            "unknown" => Ok(Self::Unknown),
            other => Err(SynapseedError::Internal(format!(
                "Unsupported language: {other}"
            ))),
        }
    }
}
