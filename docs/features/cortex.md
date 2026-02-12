# Cortex — AST Engine

The **Cortex** is SYNAPSEED's semantic brain. It replaces blind `cat` and `grep` with structured AST parsing, giving the LLM a true understanding of code structure.

## What It Does

- Parses source files into Abstract Syntax Trees using **tree-sitter**
- Extracts symbols: functions, methods, structs, classes, enums, modules, interfaces, variables, constants, imports
- Builds a `CodeGraph` with file-to-symbol relationships
- Supports **Rust**, **Python**, and **JavaScript**

## How It Works

```
Source File (.rs, .py, .js)
  → tree-sitter parser
  → AST traversal
  → Symbol extraction (name, kind, line range, signature)
  → CodeGraph (files → symbols → relationships)
```

## Key Types

### `Symbol`

```rust
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub line_start: usize,
    pub line_end: usize,
    pub signature: Option<String>,
}
```

### `FileStructure`

```rust
pub struct FileStructure {
    pub path: String,
    pub language: String,
    pub symbols: Vec<Symbol>,
}
```

### `SymbolKind`

```rust
pub enum SymbolKind {
    Function, Method, Struct, Class, Enum,
    Module, Import, Variable, Constant, Interface,
}
```

## MCP Integration

| Tool | Description |
| :--- | :--- |
| `get_code_skeleton` | Index a directory and return the full AST skeleton |
| `lookup_symbol` | Find a symbol by name across the project |

## Usage Example

```bash
# CLI mode
synapseed hoist --project ./my-project
synapseed lookup --project . "authenticate"

# MCP tool call
{"method": "tools/call", "params": {"name": "get_code_skeleton", "arguments": {"path": "."}}}
```

## Language Support

| Language | Parser | Extracted Symbols |
| :--- | :--- | :--- |
| Rust | tree-sitter-rust 0.23 | functions, methods, structs, enums, traits, impls, modules, constants |
| Python | tree-sitter-python 0.23 | functions, classes, methods, imports, variables |
| JavaScript | tree-sitter-javascript 0.23 | functions, classes, methods, imports, variables, arrow functions |
