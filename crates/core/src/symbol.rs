use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a code symbol within the project graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SymbolId(Uuid);

impl SymbolId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SymbolId {
    fn default() -> Self {
        Self::new()
    }
}

/// Visibility level of a code symbol, extracted from the AST.
///
/// Used by the Visibility Boost (v4.9.0) to prioritize public API symbols
/// over internal implementation details in search ranking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    /// `pub` — fully public API
    Public,
    /// `pub(crate)` — crate-internal
    Crate,
    /// `pub(super)` — parent-module-visible
    Super,
    /// No visibility modifier — private to the current module
    Private,
}

/// The kind of code symbol extracted from the AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Interface,
    Module,
    Import,
    Variable,
    Constant,
}

/// A code symbol — a named, located entity in the project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub id: SymbolId,
    pub name: String,
    pub kind: SymbolKind,
    pub file_path: String,
    pub line_start: usize,
    pub line_end: usize,
    pub signature: Option<String>,
    pub visibility: Option<Visibility>,
    pub children: Vec<SymbolId>,
}

/// The skeleton of a file: its symbols without source bodies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStructure {
    pub path: String,
    pub language: String,
    pub symbols: Vec<Symbol>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_id_uniqueness() {
        let a = SymbolId::new();
        let b = SymbolId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn symbol_id_default() {
        let a = SymbolId::default();
        let b = SymbolId::default();
        assert_ne!(a, b);
    }

    #[test]
    fn visibility_serde_roundtrip() {
        for vis in [
            Visibility::Public,
            Visibility::Crate,
            Visibility::Super,
            Visibility::Private,
        ] {
            let json = serde_json::to_string(&vis).unwrap();
            let back: Visibility = serde_json::from_str(&json).unwrap();
            assert_eq!(vis, back);
        }
    }

    #[test]
    fn visibility_serde_snake_case() {
        assert_eq!(serde_json::to_string(&Visibility::Public).unwrap(), "\"public\"");
        assert_eq!(serde_json::to_string(&Visibility::Crate).unwrap(), "\"crate\"");
        assert_eq!(serde_json::to_string(&Visibility::Super).unwrap(), "\"super\"");
        assert_eq!(serde_json::to_string(&Visibility::Private).unwrap(), "\"private\"");
    }

    #[test]
    fn symbol_kind_serde_roundtrip() {
        for kind in [
            SymbolKind::Function,
            SymbolKind::Method,
            SymbolKind::Class,
            SymbolKind::Struct,
            SymbolKind::Enum,
            SymbolKind::Interface,
            SymbolKind::Module,
            SymbolKind::Import,
            SymbolKind::Variable,
            SymbolKind::Constant,
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            let back: SymbolKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, back);
        }
    }

    #[test]
    fn symbol_serde_with_visibility() {
        let sym = Symbol {
            id: SymbolId::new(),
            name: "test_fn".into(),
            kind: SymbolKind::Function,
            file_path: "src/lib.rs".into(),
            line_start: 1,
            line_end: 5,
            signature: Some("pub fn test_fn()".into()),
            visibility: Some(Visibility::Public),
            children: Vec::new(),
        };
        let json = serde_json::to_string(&sym).unwrap();
        assert!(json.contains("\"public\""));
        let back: Symbol = serde_json::from_str(&json).unwrap();
        assert_eq!(back.visibility, Some(Visibility::Public));
    }

    #[test]
    fn symbol_serde_without_visibility() {
        let sym = Symbol {
            id: SymbolId::new(),
            name: "old_sym".into(),
            kind: SymbolKind::Struct,
            file_path: "src/lib.rs".into(),
            line_start: 1,
            line_end: 3,
            signature: None,
            visibility: None,
            children: Vec::new(),
        };
        let json = serde_json::to_string(&sym).unwrap();
        let back: Symbol = serde_json::from_str(&json).unwrap();
        assert_eq!(back.visibility, None);
    }

    #[test]
    fn file_structure_serde_roundtrip() {
        let fs = FileStructure {
            path: "src/main.rs".into(),
            language: "rust".into(),
            symbols: vec![],
        };
        let json = serde_json::to_string(&fs).unwrap();
        let back: FileStructure = serde_json::from_str(&json).unwrap();
        assert_eq!(back.path, "src/main.rs");
        assert_eq!(back.language, "rust");
    }
}
