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
    /// Lowercase name -> SymbolIds for O(1) case-insensitive lookup
    lowercase_index: DashMap<String, Vec<SymbolId>>,
}

impl CodeGraph {
    pub fn new() -> Self {
        Self {
            files: DashMap::new(),
            symbol_index: DashMap::new(),
            lowercase_index: DashMap::new(),
        }
    }

    /// Index a single file into the graph.
    pub fn index_file(&self, parser: &mut AstParser, path: &Path, source: &str) -> Result<()> {
        let mut structure = parser.parse_file(path, source)?;

        // Fill in file_path for all symbols and build both indices
        for (i, sym) in structure.symbols.iter_mut().enumerate() {
            sym.file_path = path.display().to_string();
            self.symbol_index.insert(sym.id, (path.to_path_buf(), i));
            // Secondary index: lowercase name -> SymbolIds
            self.lowercase_index
                .entry(sym.name.to_ascii_lowercase())
                .or_default()
                .push(sym.id);
        }

        self.files.insert(path.to_path_buf(), structure);
        Ok(())
    }

    /// Get the structural skeleton of a file (HOIST operation).
    pub fn hoist(&self, path: &Path) -> Option<FileStructure> {
        self.files.get(path).map(|entry| entry.value().clone())
    }

    /// Look up a symbol by name across all indexed files (case-insensitive, O(1)).
    pub fn lookup(&self, name: &str) -> Vec<Symbol> {
        let key = name.to_ascii_lowercase();
        match self.lowercase_index.get(&key) {
            Some(ids) => ids.iter().filter_map(|id| self.get_symbol(id)).collect(),
            None => Vec::new(),
        }
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

    /// Remove a file and all its symbols from the graph (D14 fix).
    ///
    /// Atomically removes the file's `FileStructure` from the `files` map,
    /// then cleans up both `symbol_index` and `lowercase_index` for every
    /// symbol that belonged to that file.  Safe to call concurrently with
    /// reads — `DashMap` provides per-shard locking.
    ///
    /// Returns the number of symbols removed, or 0 if the file was not indexed.
    pub fn remove_file(&self, path: &Path) -> usize {
        let structure = match self.files.remove(path) {
            Some((_, s)) => s,
            None => return 0,
        };

        let mut removed = 0usize;
        for sym in &structure.symbols {
            // Remove from primary symbol index
            self.symbol_index.remove(&sym.id);

            // Remove from lowercase index and purge empty entries (D71 fix)
            let key = sym.name.to_ascii_lowercase();
            if let Some(mut ids) = self.lowercase_index.get_mut(&key) {
                ids.retain(|id| *id != sym.id);
                if ids.is_empty() {
                    drop(ids); // release write guard before removing the key
                    self.lowercase_index.remove(&key);
                }
            }
            removed += 1;
        }

        removed
    }

    /// Purge all empty entries from `lowercase_index` (D71 fix).
    ///
    /// Scans every entry and removes keys whose `Vec<SymbolId>` is empty.
    /// Call periodically (e.g. after bulk re-indexing) to reclaim memory
    /// from phantom entries that accumulated during incremental updates.
    pub fn compact_lowercase_index(&self) -> usize {
        let empty_keys: Vec<String> = self
            .lowercase_index
            .iter()
            .filter(|entry| entry.value().is_empty())
            .map(|entry| entry.key().clone())
            .collect();
        let purged = empty_keys.len();
        for key in empty_keys {
            // Re-check under write lock — another thread may have inserted
            self.lowercase_index.remove_if(&key, |_, v| v.is_empty());
        }
        purged
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

        // Thread-local parser reuse (v5.0.0 — "Il Riciclatore"):
        // AstParser::new() initializes 3 tree-sitter grammars (~2ms each).
        // Previously we created a NEW parser per file in par_iter — O(n) grammar inits.
        // Now each rayon thread initializes ONE parser and reuses it across all
        // files assigned to that thread: O(num_threads) grammar inits instead of O(files).
        use std::cell::RefCell;
        thread_local! {
            static THREAD_PARSER: RefCell<Option<AstParser>> = const { RefCell::new(None) };
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

            THREAD_PARSER.with(|cell| {
                let mut borrow = cell.borrow_mut();
                if borrow.is_none() {
                    *borrow = match AstParser::new() {
                        Ok(p) => Some(p),
                        Err(e) => {
                            debug!(error = %e, "Failed to create parser in worker thread");
                            return;
                        }
                    };
                }
                let Some(parser) = borrow.as_mut() else {
                    return;
                };

                let parse_start = Instant::now();
                if let Err(e) = self.index_file(parser, path, &source) {
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
        });

        info!(
            files = self.file_count(),
            symbols = self.symbol_count(),
            "Code graph indexed"
        );

        // D6 fix: propagate transitive inheritance after all files are indexed
        self.propagate_inheritance();

        Ok(())
    }

    pub fn index_directory(&self, root: &Path) -> Result<()> {
        self.index_directory_with_ceiling(root, None)
    }

    /// Propagate transitive inheritance through the symbol graph.
    ///
    /// After indexing, signatures contain single-level `[inherits: X]` tags.
    /// This pass resolves the full inheritance chain so that a class inheriting
    /// from `B` which inherits from `A` will have `[inherits: B, A]` in its
    /// signature, enabling BM25 to boost the class when searching for any
    /// ancestor name.
    fn propagate_inheritance(&self) {
        use std::collections::{HashMap, HashSet};

        // Phase 1: Build class_name → direct_parents map from all symbols
        let mut class_parents: HashMap<String, Vec<String>> = HashMap::new();
        for entry in self.files.iter() {
            for sym in &entry.value().symbols {
                if let Some(ref sig) = sym.signature {
                    if let Some(parents) = Self::parse_inherits_tag(sig) {
                        class_parents.insert(sym.name.clone(), parents);
                    }
                }
            }
        }

        if class_parents.is_empty() {
            return;
        }

        // Phase 2: Compute transitive closure for each class
        let mut transitive: HashMap<String, Vec<String>> = HashMap::new();
        for class_name in class_parents.keys() {
            let mut all_ancestors = Vec::new();
            let mut visited = HashSet::new();
            let mut queue = std::collections::VecDeque::new();

            // Seed with direct parents
            if let Some(direct) = class_parents.get(class_name) {
                for p in direct {
                    if visited.insert(p.clone()) {
                        queue.push_back(p.clone());
                        all_ancestors.push(p.clone());
                    }
                }
            }

            // BFS up the inheritance chain (cap at 20 to avoid cycles)
            let mut depth = 0;
            while let Some(ancestor) = queue.pop_front() {
                depth += 1;
                if depth > 20 {
                    warn!(class = %class_name, "Inheritance chain exceeds 20 levels, truncating");
                    break;
                }
                if let Some(grandparents) = class_parents.get(&ancestor) {
                    for gp in grandparents {
                        if visited.insert(gp.clone()) {
                            queue.push_back(gp.clone());
                            all_ancestors.push(gp.clone());
                        }
                    }
                }
            }

            // Only store if we found additional ancestors beyond direct parents
            if let Some(direct) = class_parents.get(class_name) {
                if all_ancestors.len() > direct.len() {
                    transitive.insert(class_name.clone(), all_ancestors);
                }
            }
        }

        if transitive.is_empty() {
            return;
        }

        // Phase 3: Update signatures with transitive parents
        let mut updated = 0usize;
        for mut entry in self.files.iter_mut() {
            for sym in entry.value_mut().symbols.iter_mut() {
                if let Some(ancestors) = transitive.get(&sym.name) {
                    if let Some(ref mut sig) = sym.signature {
                        // Replace existing [inherits: ...] tag with full transitive list
                        if let Some(start) = sig.find("[inherits: ") {
                            if let Some(end) = sig[start..].find(']') {
                                let new_tag = format!("[inherits: {}]", ancestors.join(", "));
                                sig.replace_range(start..start + end + 1, &new_tag);
                                updated += 1;
                            }
                        }
                    }
                }
            }
        }

        if updated > 0 {
            debug!(classes = updated, "Propagated transitive inheritance");
        }
    }

    /// Parse `[inherits: X, Y]` tag from a signature string.
    fn parse_inherits_tag(sig: &str) -> Option<Vec<String>> {
        let start = sig.find("[inherits: ")?;
        let rest = &sig[start + 11..]; // skip "[inherits: "
        let end = rest.find(']')?;
        let names = &rest[..end];
        let parents: Vec<String> = names
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if parents.is_empty() {
            None
        } else {
            Some(parents)
        }
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
        if entry.file_type().is_none_or(|ft| !ft.is_file()) {
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
