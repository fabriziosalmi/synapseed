use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use ignore::WalkBuilder;
use rayon::prelude::*;
use synapseed_core::error::Result;
use synapseed_core::symbol::{FileStructure, Symbol, SymbolId};
use tracing::{debug, info, warn};

use crate::parser::AstParser;

/// Maximum file size to index (1 MB). Files larger than this are skipped
/// to prevent OOM on generated code, vendored deps, and binary files.
const MAX_FILE_SIZE: u64 = 1_048_576;

/// Per-file parse time warning threshold (5 seconds).
/// Files exceeding this are logged and counted.
const PARSE_TIMEOUT: Duration = Duration::from_secs(5);

/// Default maximum files to index (HCI Req 6: Silent Partner / memory ceiling).
const DEFAULT_MAX_FILES: usize = 10_000;

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
            self.symbol_index.insert(sym.id, (path.to_path_buf(), i));
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
        self.files
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Index all supported files in a directory tree.
    ///
    /// Uses rayon for parallel parsing — each thread creates its own
    /// `AstParser` since tree-sitter `Parser` requires `&mut self`.
    /// Thread-safety is guaranteed by `DashMap`.
    /// Index all supported files in a directory tree with memory ceiling.
    ///
    /// `max_files` caps the total number of files indexed (HCI Req 6: Silent Partner).
    /// Pass `None` for the default ceiling of 10,000 files.
    pub fn index_directory_with_ceiling(
        &self,
        root: &Path,
        max_files: Option<usize>,
    ) -> Result<()> {
        let mut paths = walkdir(root)?;
        let max = max_files.unwrap_or(DEFAULT_MAX_FILES);

        if paths.len() > max {
            info!(
                total = paths.len(),
                cap = max,
                "Capping indexed files to memory ceiling"
            );
            paths.truncate(max);
        }

        paths.par_iter().for_each(|path| {
            // Size guard: skip files larger than MAX_FILE_SIZE
            match std::fs::metadata(path) {
                Ok(meta) if meta.len() > MAX_FILE_SIZE => {
                    debug!(
                        path = %path.display(),
                        bytes = meta.len(),
                        "Skipping oversized file (>{} bytes)",
                        MAX_FILE_SIZE
                    );
                    return;
                }
                Err(e) => {
                    debug!(path = %path.display(), error = %e, "Cannot stat file, skipping");
                    return;
                }
                _ => {}
            }

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

            let parse_start = Instant::now();
            if let Err(e) = self.index_file(&mut parser, path, &source) {
                debug!(path = %path.display(), error = %e, "Skipping file");
            }
            let elapsed = parse_start.elapsed();
            if elapsed > PARSE_TIMEOUT {
                warn!(
                    path = %path.display(),
                    ms = elapsed.as_millis(),
                    "Parser exceeded timeout threshold"
                );
            }
        });

        info!(
            files = self.file_count(),
            symbols = self.symbol_count(),
            "Code graph indexed"
        );

        Ok(())
    }

    pub fn index_directory(&self, root: &Path) -> Result<()> {
        self.index_directory_with_ceiling(root, None)
    }
}

impl Default for CodeGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Walk a directory using the `ignore` crate — respects .gitignore,
/// .ignore, and global gitignore. Automatically skips hidden dirs,
/// target/, node_modules/, etc. via gitignore rules.
fn walkdir(root: &Path) -> Result<Vec<PathBuf>> {
    let walker = WalkBuilder::new(root)
        .hidden(true) // respect hidden dirs (skip .* dirs)
        .git_ignore(true) // respect .gitignore
        .git_global(true) // respect global gitignore
        .git_exclude(true) // respect .git/info/exclude
        .build();

    let mut paths = Vec::new();
    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                warn!(error = %e, "File walk error, skipping entry");
                continue;
            }
        };

        // Skip directories (we only want files)
        if entry.file_type().map_or(true, |ft| !ft.is_file()) {
            continue;
        }

        let path = entry.into_path();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if crate::language::Language::from_extension(ext).is_some() {
                paths.push(path);
            }
        }
    }

    Ok(paths)
}
