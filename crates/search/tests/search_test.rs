//! Integration tests for the synapseed-search crate.
//!
//! Tests SemanticIndex creation, indexing, searching, re-indexing,
//! and edge cases (empty index, disk persistence).

use std::path::Path;

use synapseed_core::symbol::{FileStructure, Symbol, SymbolId, SymbolKind};
use synapseed_search::indexer::SemanticIndex;

/// Helper: build a FileStructure with a single function symbol.
fn make_file(path: &str, name: &str, signature: &str) -> FileStructure {
    FileStructure {
        path: path.to_string(),
        language: "rust".to_string(),
        symbols: vec![Symbol {
            id: SymbolId::new(),
            name: name.to_string(),
            kind: SymbolKind::Function,
            file_path: path.to_string(),
            line_start: 1,
            line_end: 10,
            signature: Some(signature.to_string()),
            visibility: None,
            children: Vec::new(),
        }],
    }
}

/// Helper: build a FileStructure with multiple symbols.
fn make_file_multi(path: &str, symbols: Vec<(&str, SymbolKind, &str)>) -> FileStructure {
    FileStructure {
        path: path.to_string(),
        language: "rust".to_string(),
        symbols: symbols
            .into_iter()
            .enumerate()
            .map(|(i, (name, kind, sig))| Symbol {
                id: SymbolId::new(),
                name: name.to_string(),
                kind,
                file_path: path.to_string(),
                line_start: i * 10 + 1,
                line_end: (i + 1) * 10,
                signature: Some(sig.to_string()),
                visibility: None,
                children: Vec::new(),
            })
            .collect(),
    }
}

// ── Index Creation ──────────────────────────────────────────────────

#[test]
fn create_in_memory_index() {
    let index = SemanticIndex::new();
    assert!(index.is_ok(), "In-memory index creation should succeed");
}

#[test]
fn create_disk_index_in_tempdir() {
    let tmp = tempfile::tempdir().expect("Failed to create tempdir");
    let index_dir = tmp.path().join("test_index");
    let index = SemanticIndex::open_or_create(&index_dir);
    assert!(
        index.is_ok(),
        "Disk index creation should succeed, got: {:?}",
        index.err()
    );
}

// ── Indexing and Searching ──────────────────────────────────────────

#[test]
fn index_file_and_search_by_name() {
    let index = SemanticIndex::new().unwrap();
    let project_root = Path::new("/nonexistent");

    let file = make_file(
        "src/auth.rs",
        "authenticate_user",
        "fn authenticate_user(creds: &Credentials) -> Result<Token>",
    );
    let count = index.index_all(&[file], project_root);
    assert_eq!(count, 1, "Should have indexed 1 symbol");

    // Wait for the reader to reload (OnCommitWithDelay adds ~500ms)
    std::thread::sleep(std::time::Duration::from_millis(700));

    let results = index.search("authenticate", 10);
    assert!(
        !results.is_empty(),
        "Search for 'authenticate' should return results"
    );
    assert_eq!(results[0].symbol, "authenticate_user");
    assert_eq!(results[0].file, "src/auth.rs");
}

#[test]
fn search_empty_index_returns_zero_results() {
    let index = SemanticIndex::new().unwrap();
    let results = index.search("anything", 10);
    assert!(results.is_empty(), "Empty index should return zero results");
}

#[test]
fn index_multiple_files_and_search() {
    let index = SemanticIndex::new().unwrap();
    let project_root = Path::new("/nonexistent");

    let files = vec![
        make_file(
            "src/auth.rs",
            "login",
            "fn login(user: &str, pass: &str) -> bool",
        ),
        make_file(
            "src/db.rs",
            "connect_database",
            "fn connect_database(url: &str) -> Connection",
        ),
        make_file(
            "src/api.rs",
            "handle_request",
            "fn handle_request(req: Request) -> Response",
        ),
    ];
    let count = index.index_all(&files, project_root);
    assert_eq!(count, 3, "Should have indexed 3 symbols");

    std::thread::sleep(std::time::Duration::from_millis(700));

    let results = index.search("database connection", 10);
    assert!(
        !results.is_empty(),
        "Search for 'database connection' should return results"
    );
    // The database-related symbol should rank highly
    assert!(
        results.iter().any(|r| r.symbol == "connect_database"),
        "connect_database should appear in results"
    );
}

// ── Re-indexing (No Duplicates) ─────────────────────────────────────

#[test]
fn reindex_same_file_no_duplicates() {
    let index = SemanticIndex::new().unwrap();
    let project_root = Path::new("/nonexistent");

    let file = make_file(
        "src/lib.rs",
        "process_data",
        "fn process_data(input: &[u8]) -> Vec<u8>",
    );

    // Index the file twice using reindex_file (delete + re-add)
    index.index_all(std::slice::from_ref(&file), project_root);
    std::thread::sleep(std::time::Duration::from_millis(700));

    let count = index.reindex_file(&file, project_root);
    assert_eq!(count, 1, "Reindex should add 1 symbol");

    std::thread::sleep(std::time::Duration::from_millis(700));

    let results = index.search("process_data", 10);
    assert_eq!(
        results.len(),
        1,
        "After reindex, should have exactly 1 result, got {}",
        results.len()
    );
}

// ── Remove File ─────────────────────────────────────────────────────

#[test]
fn remove_file_from_index() {
    let index = SemanticIndex::new().unwrap();
    let project_root = Path::new("/nonexistent");

    let file = make_file("src/temp.rs", "temp_function", "fn temp_function()");
    index.index_all(&[file], project_root);
    std::thread::sleep(std::time::Duration::from_millis(700));

    // Verify it's indexed
    let results = index.search("temp_function", 10);
    assert!(
        !results.is_empty(),
        "Should find temp_function before removal"
    );

    // Remove it
    index.remove_file("src/temp.rs");
    std::thread::sleep(std::time::Duration::from_millis(700));

    let results = index.search("temp_function", 10);
    assert!(
        results.is_empty(),
        "After removal, temp_function should not appear in results"
    );
}

// ── Temporal Decay ──────────────────────────────────────────────────

#[test]
fn temporal_decay_setting() {
    let mut index = SemanticIndex::new().unwrap();
    // Setting decay should not panic
    index.set_temporal_decay(0.1);
    index.set_temporal_decay(0.0);
    index.set_temporal_decay(1.0);
}

// ── Multi-symbol File ───────────────────────────────────────────────

#[test]
fn index_file_with_multiple_symbols() {
    let index = SemanticIndex::new().unwrap();
    let project_root = Path::new("/nonexistent");

    let file = make_file_multi(
        "src/service.rs",
        vec![
            ("UserService", SymbolKind::Struct, "struct UserService"),
            (
                "create_user",
                SymbolKind::Method,
                "fn create_user(&self, name: &str) -> User",
            ),
            (
                "delete_user",
                SymbolKind::Method,
                "fn delete_user(&self, id: u64) -> bool",
            ),
            (
                "list_users",
                SymbolKind::Method,
                "fn list_users(&self) -> Vec<User>",
            ),
        ],
    );

    let count = index.index_all(&[file], project_root);
    assert_eq!(count, 4, "Should have indexed 4 symbols");

    std::thread::sleep(std::time::Duration::from_millis(700));

    let results = index.search("create_user", 10);
    assert!(
        !results.is_empty(),
        "Search for 'create_user' should return results"
    );
}
