//! Cortex Graph tests — CodeGraph creation, indexing, symbol lookup,
//! multi-language parsing, and edge cases.

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use synapseed_cortex::graph::CodeGraph;
use synapseed_cortex::parser::AstParser;

/// Helper: create a file inside a temp directory.
fn write_file(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, content).unwrap();
}

// ══════════════════════════════════════════════════════════════
// 1. Empty graph
// ══════════════════════════════════════════════════════════════

#[test]
fn test_new_graph_is_empty() {
    let graph = CodeGraph::new();
    assert_eq!(graph.file_count(), 0);
    assert_eq!(graph.symbol_count(), 0);
    assert!(graph.all_files().is_empty());
}

#[test]
fn test_empty_directory_produces_empty_graph() {
    let dir = TempDir::new().unwrap();
    let graph = CodeGraph::new();
    graph.index_directory(dir.path()).unwrap();
    assert_eq!(graph.file_count(), 0);
    assert_eq!(graph.symbol_count(), 0);
}

// ══════════════════════════════════════════════════════════════
// 2. Single Rust file indexing
// ══════════════════════════════════════════════════════════════

#[test]
fn test_index_single_rust_file() {
    let dir = TempDir::new().unwrap();
    write_file(
        dir.path(),
        "lib.rs",
        r#"
pub fn hello() -> String {
    "hello".to_string()
}

pub struct Config {
    pub name: String,
}

pub enum Status {
    Active,
    Inactive,
}
"#,
    );

    let graph = CodeGraph::new();
    let mut parser = AstParser::new().unwrap();
    let source = fs::read_to_string(dir.path().join("lib.rs")).unwrap();
    graph
        .index_file(&mut parser, &dir.path().join("lib.rs"), &source)
        .unwrap();

    assert_eq!(graph.file_count(), 1);
    assert!(graph.symbol_count() >= 3, "Should find at least fn, struct, enum");
}

#[test]
fn test_lookup_found() {
    let dir = TempDir::new().unwrap();
    write_file(
        dir.path(),
        "main.rs",
        r#"
fn main() {
    println!("hello");
}

fn helper() -> bool {
    true
}
"#,
    );

    let graph = CodeGraph::new();
    let mut parser = AstParser::new().unwrap();
    let source = fs::read_to_string(dir.path().join("main.rs")).unwrap();
    graph
        .index_file(&mut parser, &dir.path().join("main.rs"), &source)
        .unwrap();

    let results = graph.lookup("main");
    assert!(!results.is_empty(), "Should find the 'main' symbol");
    assert_eq!(results[0].name, "main");
}

#[test]
fn test_lookup_not_found() {
    let dir = TempDir::new().unwrap();
    write_file(dir.path(), "lib.rs", "pub fn foo() {}");

    let graph = CodeGraph::new();
    let mut parser = AstParser::new().unwrap();
    let source = fs::read_to_string(dir.path().join("lib.rs")).unwrap();
    graph
        .index_file(&mut parser, &dir.path().join("lib.rs"), &source)
        .unwrap();

    let results = graph.lookup("nonexistent_symbol");
    assert!(results.is_empty(), "Should not find a nonexistent symbol");
}

// ══════════════════════════════════════════════════════════════
// 3. Hoist (file skeleton retrieval)
// ══════════════════════════════════════════════════════════════

#[test]
fn test_hoist_returns_file_structure() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("module.rs");
    write_file(
        dir.path(),
        "module.rs",
        r#"
pub fn alpha() {}
pub fn beta() {}
"#,
    );

    let graph = CodeGraph::new();
    let mut parser = AstParser::new().unwrap();
    let source = fs::read_to_string(&file_path).unwrap();
    graph.index_file(&mut parser, &file_path, &source).unwrap();

    let structure = graph.hoist(&file_path);
    assert!(structure.is_some(), "Hoist should return the indexed file");
    let structure = structure.unwrap();
    assert_eq!(structure.language, "rust");
    assert!(
        structure.symbols.len() >= 2,
        "Should have at least alpha and beta symbols"
    );
}

#[test]
fn test_hoist_nonexistent_file_returns_none() {
    let graph = CodeGraph::new();
    let result = graph.hoist(Path::new("/nonexistent/file.rs"));
    assert!(result.is_none());
}

// ══════════════════════════════════════════════════════════════
// 4. Multi-language parsing
// ══════════════════════════════════════════════════════════════

#[test]
fn test_index_python_file() {
    let dir = TempDir::new().unwrap();
    write_file(
        dir.path(),
        "app.py",
        r#"
def greet(name):
    return f"Hello, {name}!"

class Server:
    def __init__(self):
        self.running = False

    def start(self):
        self.running = True
"#,
    );

    let graph = CodeGraph::new();
    let mut parser = AstParser::new().unwrap();
    let source = fs::read_to_string(dir.path().join("app.py")).unwrap();
    graph
        .index_file(&mut parser, &dir.path().join("app.py"), &source)
        .unwrap();

    assert_eq!(graph.file_count(), 1);

    // Should find the function and class
    let greet = graph.lookup("greet");
    assert!(!greet.is_empty(), "Should find Python function 'greet'");

    let server = graph.lookup("Server");
    assert!(!server.is_empty(), "Should find Python class 'Server'");

    let structure = graph.hoist(&dir.path().join("app.py")).unwrap();
    assert_eq!(structure.language, "python");
}

#[test]
fn test_index_javascript_file() {
    let dir = TempDir::new().unwrap();
    write_file(
        dir.path(),
        "index.js",
        r#"
function renderApp() {
    console.log("rendering");
}

class Component {
    constructor() {
        this.state = {};
    }
}
"#,
    );

    let graph = CodeGraph::new();
    let mut parser = AstParser::new().unwrap();
    let source = fs::read_to_string(dir.path().join("index.js")).unwrap();
    graph
        .index_file(&mut parser, &dir.path().join("index.js"), &source)
        .unwrap();

    assert_eq!(graph.file_count(), 1);

    let render = graph.lookup("renderApp");
    assert!(!render.is_empty(), "Should find JS function 'renderApp'");

    let component = graph.lookup("Component");
    assert!(!component.is_empty(), "Should find JS class 'Component'");

    let structure = graph.hoist(&dir.path().join("index.js")).unwrap();
    assert_eq!(structure.language, "javascript");
}

// ══════════════════════════════════════════════════════════════
// 5. Directory indexing
// ══════════════════════════════════════════════════════════════

#[test]
fn test_index_directory_with_multiple_files() {
    let dir = TempDir::new().unwrap();
    write_file(dir.path(), "src/lib.rs", "pub fn lib_fn() {}");
    write_file(dir.path(), "src/util.rs", "pub fn util_fn() {}");
    write_file(dir.path(), "app.py", "def py_fn(): pass");

    let graph = CodeGraph::new();
    graph.index_directory(dir.path()).unwrap();

    assert!(
        graph.file_count() >= 3,
        "Should index at least 3 files, got {}",
        graph.file_count()
    );

    assert!(!graph.lookup("lib_fn").is_empty(), "Should find lib_fn");
    assert!(!graph.lookup("util_fn").is_empty(), "Should find util_fn");
    assert!(!graph.lookup("py_fn").is_empty(), "Should find py_fn");
}

#[test]
fn test_index_directory_with_ceiling() {
    let dir = TempDir::new().unwrap();
    // Create more files than the ceiling
    for i in 0..5 {
        write_file(dir.path(), &format!("file_{i}.rs"), &format!("pub fn func_{i}() {{}}"));
    }

    let graph = CodeGraph::new();
    graph
        .index_directory_with_ceiling(dir.path(), Some(2))
        .unwrap();

    // With a ceiling of 2, at most 2 files should be indexed
    assert!(
        graph.file_count() <= 2,
        "Ceiling should limit files to 2, got {}",
        graph.file_count()
    );
}

// ══════════════════════════════════════════════════════════════
// 6. Edge cases
// ══════════════════════════════════════════════════════════════

#[test]
fn test_file_with_syntax_errors_graceful() {
    let dir = TempDir::new().unwrap();
    // Invalid Rust syntax — tree-sitter should still produce a partial parse
    write_file(
        dir.path(),
        "broken.rs",
        r#"
fn incomplete(
    // missing closing paren and body
pub struct Orphan {
}
"#,
    );

    let graph = CodeGraph::new();
    let mut parser = AstParser::new().unwrap();
    let source = fs::read_to_string(dir.path().join("broken.rs")).unwrap();

    // Should not panic — tree-sitter handles partial parses gracefully
    let result = graph.index_file(&mut parser, &dir.path().join("broken.rs"), &source);
    assert!(
        result.is_ok(),
        "Indexing a file with syntax errors should not fail: {:?}",
        result.err()
    );
}

#[test]
fn test_symbol_has_file_path_filled() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("check.rs");
    write_file(dir.path(), "check.rs", "pub fn my_func() {}");

    let graph = CodeGraph::new();
    let mut parser = AstParser::new().unwrap();
    let source = fs::read_to_string(&file_path).unwrap();
    graph.index_file(&mut parser, &file_path, &source).unwrap();

    let results = graph.lookup("my_func");
    assert!(!results.is_empty());
    assert!(
        !results[0].file_path.is_empty(),
        "Symbol file_path should be populated"
    );
    assert!(
        results[0].file_path.contains("check.rs"),
        "file_path should contain the file name"
    );
}

#[test]
fn test_get_symbol_by_id() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("id_test.rs");
    write_file(dir.path(), "id_test.rs", "pub fn target_fn() {}");

    let graph = CodeGraph::new();
    let mut parser = AstParser::new().unwrap();
    let source = fs::read_to_string(&file_path).unwrap();
    graph.index_file(&mut parser, &file_path, &source).unwrap();

    let results = graph.lookup("target_fn");
    assert!(!results.is_empty());
    let id = results[0].id;

    let by_id = graph.get_symbol(&id);
    assert!(by_id.is_some(), "get_symbol should find the symbol by ID");
    assert_eq!(by_id.unwrap().name, "target_fn");
}

// ══════════════════════════════════════════════════════════════
// 7. Trait Expansion (v4.2.0)
// ══════════════════════════════════════════════════════════════

#[test]
fn test_trait_item_indexed_as_interface() {
    let dir = TempDir::new().unwrap();
    write_file(
        dir.path(),
        "traits.rs",
        r#"
pub trait Logger {
    fn log(&self, msg: &str);
}

pub struct FileLogger;

impl Logger for FileLogger {
    fn log(&self, msg: &str) {
        println!("{}", msg);
    }
}
"#,
    );

    let graph = CodeGraph::new();
    let mut parser = AstParser::new().unwrap();
    let source = fs::read_to_string(dir.path().join("traits.rs")).unwrap();
    graph
        .index_file(&mut parser, &dir.path().join("traits.rs"), &source)
        .unwrap();

    // trait_item → SymbolKind::Interface, indexed by name "Logger"
    let logger_trait = graph.lookup("Logger");
    assert!(!logger_trait.is_empty(), "Should find trait 'Logger'");

    // impl Logger for FileLogger → indexed by type name "FileLogger"
    let file_logger = graph.lookup("FileLogger");
    assert!(
        !file_logger.is_empty(),
        "Should find 'FileLogger' from both struct and impl"
    );

    // The impl symbol should have the trait reference in its signature
    let impl_sym = file_logger
        .iter()
        .find(|s| s.signature.as_deref().unwrap_or("").contains("trait:"))
        .expect("One FileLogger symbol should have trait reference in signature");
    assert!(
        impl_sym.signature.as_deref().unwrap().contains("Logger"),
        "Signature should contain trait name 'Logger'"
    );
}

#[test]
fn test_inherent_impl_no_trait_ref() {
    let dir = TempDir::new().unwrap();
    write_file(
        dir.path(),
        "inherent.rs",
        r#"
pub struct Counter {
    value: u32,
}

impl Counter {
    pub fn new() -> Self {
        Counter { value: 0 }
    }
}
"#,
    );

    let graph = CodeGraph::new();
    let mut parser = AstParser::new().unwrap();
    let source = fs::read_to_string(dir.path().join("inherent.rs")).unwrap();
    graph
        .index_file(&mut parser, &dir.path().join("inherent.rs"), &source)
        .unwrap();

    let counter = graph.lookup("Counter");
    assert!(!counter.is_empty(), "Should find 'Counter'");

    // Inherent impl should NOT have "[trait: ...]" in signature
    for sym in &counter {
        if let Some(ref sig) = sym.signature {
            assert!(
                !sig.contains("trait:"),
                "Inherent impl should not have trait reference: {sig}"
            );
        }
    }
}

// ══════════════════════════════════════════════════════════════
// 8. Inheritance Boost — Python (v4.4.0)
// ══════════════════════════════════════════════════════════════

#[test]
fn test_python_class_inheritance_simple() {
    let dir = TempDir::new().unwrap();
    write_file(
        dir.path(),
        "middleware.py",
        r#"
class MiddlewareMixin:
    def process_request(self, request):
        pass

class SecurityMiddleware(MiddlewareMixin):
    def process_request(self, request):
        return self.check_security(request)
"#,
    );

    let graph = CodeGraph::new();
    let mut parser = AstParser::new().unwrap();
    let source = fs::read_to_string(dir.path().join("middleware.py")).unwrap();
    graph
        .index_file(&mut parser, &dir.path().join("middleware.py"), &source)
        .unwrap();

    // SecurityMiddleware should have [inherits: MiddlewareMixin] in signature
    let security = graph.lookup("SecurityMiddleware");
    assert!(!security.is_empty(), "Should find SecurityMiddleware");
    let sig = security[0].signature.as_deref().unwrap_or("");
    assert!(
        sig.contains("[inherits: MiddlewareMixin]"),
        "Signature should contain inheritance tag, got: {sig}"
    );

    // MiddlewareMixin (no parent) should NOT have [inherits: ...]
    let mixin = graph.lookup("MiddlewareMixin");
    assert!(!mixin.is_empty(), "Should find MiddlewareMixin");
    let mixin_sig = mixin[0].signature.as_deref().unwrap_or("");
    assert!(
        !mixin_sig.contains("[inherits:"),
        "Base class should not have inherits tag, got: {mixin_sig}"
    );
}

#[test]
fn test_python_class_multiple_inheritance() {
    let dir = TempDir::new().unwrap();
    write_file(
        dir.path(),
        "views.py",
        r#"
class View:
    pass

class LoginRequiredMixin:
    pass

class DashboardView(LoginRequiredMixin, View):
    def get(self, request):
        return render(request, "dashboard.html")
"#,
    );

    let graph = CodeGraph::new();
    let mut parser = AstParser::new().unwrap();
    let source = fs::read_to_string(dir.path().join("views.py")).unwrap();
    graph
        .index_file(&mut parser, &dir.path().join("views.py"), &source)
        .unwrap();

    let dashboard = graph.lookup("DashboardView");
    assert!(!dashboard.is_empty(), "Should find DashboardView");
    let sig = dashboard[0].signature.as_deref().unwrap_or("");
    assert!(
        sig.contains("LoginRequiredMixin"),
        "Signature should contain first parent: {sig}"
    );
    assert!(
        sig.contains("View"),
        "Signature should contain second parent: {sig}"
    );
}

#[test]
fn test_python_class_dotted_superclass() {
    let dir = TempDir::new().unwrap();
    write_file(
        dir.path(),
        "models.py",
        r#"
import django.db.models

class Article(django.db.models.Model):
    title = CharField(max_length=200)
"#,
    );

    let graph = CodeGraph::new();
    let mut parser = AstParser::new().unwrap();
    let source = fs::read_to_string(dir.path().join("models.py")).unwrap();
    graph
        .index_file(&mut parser, &dir.path().join("models.py"), &source)
        .unwrap();

    let article = graph.lookup("Article");
    assert!(!article.is_empty(), "Should find Article");
    let sig = article[0].signature.as_deref().unwrap_or("");
    // Dotted superclass: extracts last component "Model"
    assert!(
        sig.contains("[inherits: Model]"),
        "Should extract last component from dotted superclass: {sig}"
    );
}

#[test]
fn test_default_trait() {
    let graph = CodeGraph::default();
    assert_eq!(graph.file_count(), 0);
}
