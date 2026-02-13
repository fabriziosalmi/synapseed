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
