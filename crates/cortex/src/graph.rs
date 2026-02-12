use std::path::{Path, PathBuf};

use dashmap::DashMap;
use rayon::prelude::*;
use synapseed_core::error::Result;
use synapseed_core::symbol::{FileStructure, Symbol, SymbolId};
use tracing::{debug, info};

use crate::parser::AstParser;

/// The in-memory code graph — a semantic index of the entire project.
///
/// Thread-safe via DashMap. Supports concurrent reads and incremental
/// updates when files change.
pub struct CodeGraph {
    /// File path -> parsed structure
    files: DashMap<PathBuf, FileStructure>,
    /// Symbol ID -> (file_path, symbol index) for O(1) lookup
    symbol_index: DashMap<SymbolId, (PathBuf, usize)>,
}

impl CodeGraph {
    pub fn new() -> Self {
        Self {
            files: DashMap::new(),
            symbol_index: DashMap::new(),
        }
    }

    /// Index a single file into the graph.
    pub fn index_file(&self, parser: &mut AstParser, path: &Path, source: &str) -> Result<()> {
        let mut structure = parser.parse_file(path, source)?;

        // Fill in file_path for all symbols and build the index
        for (i, sym) in structure.symbols.iter_mut().enumerate() {
            sym.file_path = path.display().to_string();
            self.symbol_index
                .insert(sym.id, (path.to_path_buf(), i));
        }

        self.files.insert(path.to_path_buf(), structure);
        Ok(())
    }

    /// Get the structural skeleton of a file (HOIST operation).
    pub fn hoist(&self, path: &Path) -> Option<FileStructure> {
        self.files.get(path).map(|entry| entry.value().clone())
    }

    /// Look up a symbol by name across all indexed files.
    pub fn lookup(&self, name: &str) -> Vec<Symbol> {
        let mut results = Vec::new();
        for entry in self.files.iter() {
            for sym in &entry.value().symbols {
                if sym.name == name {
                    results.push(sym.clone());
                }
            }
        }
        results
    }

    /// Get a symbol by its unique ID.
    pub fn get_symbol(&self, id: &SymbolId) -> Option<Symbol> {
        let (path, idx) = self.symbol_index.get(id)?.value().clone();
        let structure = self.files.get(&path)?;
        structure.symbols.get(idx).cloned()
    }

    /// Number of indexed files.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Total number of indexed symbols.
    pub fn symbol_count(&self) -> usize {
        self.symbol_index.len()
    }

    /// Get all indexed files with their symbol structures.
    pub fn all_files(&self) -> Vec<FileStructure> {
        self.files.iter().map(|entry| entry.value().clone()).collect()
    }

    /// Index all supported files in a directory tree.
    ///
    /// Uses rayon for parallel parsing — each thread creates its own
    /// `AstParser` since tree-sitter `Parser` requires `&mut self`.
    /// Thread-safety is guaranteed by `DashMap`.
    pub fn index_directory(&self, root: &Path) -> Result<()> {
        let paths = walkdir(root)?;

        paths.par_iter().for_each(|path| {
            let source = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(_) => return, // skip binary/unreadable files
            };

            let mut parser = match AstParser::new() {
                Ok(p) => p,
                Err(e) => {
                    debug!(error = %e, "Failed to create parser in worker thread");
                    return;
                }
            };

            if let Err(e) = self.index_file(&mut parser, path, &source) {
                debug!(path = %path.display(), error = %e, "Skipping file");
            }
        });

        info!(
            files = self.file_count(),
            symbols = self.symbol_count(),
            "Code graph indexed"
        );

        Ok(())
    }
}

impl Default for CodeGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Walk a directory and collect paths with supported extensions.
fn walkdir(root: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    walk_recursive(root, &mut paths)?;
    Ok(paths)
}

fn walk_recursive(dir: &Path, paths: &mut Vec<PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(dir)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // Skip hidden dirs and common non-source dirs
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            walk_recursive(&path, paths)?;
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if crate::language::Language::from_extension(ext).is_some() {
                paths.push(path);
            }
        }
    }

    Ok(())
}
