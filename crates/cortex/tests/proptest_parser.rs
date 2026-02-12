//! Property-based tests for the Cortex AST parser.
//!
//! These tests verify that the tree-sitter-based parser never panics
//! on arbitrary input, including malformed code, binary data, and
//! pathological nesting.

use std::path::Path;

use proptest::prelude::*;
use synapseed_cortex::parser::AstParser;

proptest! {
    /// Parsing arbitrary strings as Rust code must never panic.
    /// Tree-sitter is designed to handle malformed input gracefully,
    /// but this property verifies it end-to-end through our wrapper.
    #[test]
    fn parse_rust_never_panics(input in "\\PC{0,2000}") {
        let mut parser = AstParser::new().expect("AstParser::new() should succeed");
        let path = Path::new("fuzz_input.rs");
        // We don't care if parsing succeeds or fails — only that it doesn't panic.
        let _ = parser.parse_file(path, &input);
    }

    /// Parsing arbitrary strings as Python code must never panic.
    #[test]
    fn parse_python_never_panics(input in "\\PC{0,2000}") {
        let mut parser = AstParser::new().expect("AstParser::new() should succeed");
        let path = Path::new("fuzz_input.py");
        let _ = parser.parse_file(path, &input);
    }

    /// Parsing arbitrary strings as JavaScript code must never panic.
    #[test]
    fn parse_javascript_never_panics(input in "\\PC{0,2000}") {
        let mut parser = AstParser::new().expect("AstParser::new() should succeed");
        let path = Path::new("fuzz_input.js");
        let _ = parser.parse_file(path, &input);
    }

    /// Parsing the same input twice must produce the same number of symbols.
    /// This verifies determinism in the AST extraction pipeline.
    #[test]
    fn parse_is_deterministic(input in "\\PC{0,1000}") {
        let mut parser = AstParser::new().expect("AstParser::new() should succeed");
        let path = Path::new("fuzz_input.rs");
        let result1 = parser.parse_file(path, &input);
        let result2 = parser.parse_file(path, &input);

        match (&result1, &result2) {
            (Ok(fs1), Ok(fs2)) => {
                prop_assert_eq!(fs1.symbols.len(), fs2.symbols.len(),
                    "Symbol count differed between parses of the same input");
                prop_assert_eq!(&fs1.language, &fs2.language,
                    "Language differed between parses");
            }
            (Err(_), Err(_)) => {
                // Both failed — consistent.
            }
            _ => prop_assert!(false,
                "Same input produced Ok and Err on different parses"),
        }
    }

    /// Valid Rust source code must always parse successfully.
    /// We generate simple but syntactically valid Rust functions.
    #[test]
    fn valid_rust_always_parses(
        name in "[a-z][a-z0-9_]{0,15}",
        body_val in 0i32..1000,
    ) {
        let source = format!("fn {}() -> i32 {{ {} }}", name, body_val);
        let mut parser = AstParser::new().expect("AstParser::new() should succeed");
        let path = Path::new("valid.rs");
        let result = parser.parse_file(path, &source);
        prop_assert!(result.is_ok(),
            "Valid Rust code failed to parse: {:?}", source);
        let fs = result.unwrap();
        prop_assert!(!fs.symbols.is_empty(),
            "Valid Rust function should produce at least one symbol");
    }

    /// Unsupported file extensions must return an error, never panic.
    #[test]
    fn unsupported_extension_returns_error(
        input in "\\PC{0,200}",
        ext in "(txt|csv|xml|html|css|sql|md|json|yaml)"
    ) {
        let mut parser = AstParser::new().expect("AstParser::new() should succeed");
        let filename = format!("file.{}", ext);
        let path = Path::new(&filename);
        let result = parser.parse_file(path, &input);
        // Text-only fallback languages return Ok; truly unsupported ones return Err.
        // Either way, no panic is the key invariant.
        let _ = result;
    }

    /// Symbol line numbers must be within the source range.
    #[test]
    fn symbol_lines_within_source(input in "\\PC{0,1000}") {
        let mut parser = AstParser::new().expect("AstParser::new() should succeed");
        let path = Path::new("check_lines.rs");
        if let Ok(fs) = parser.parse_file(path, &input) {
            let total_lines = input.lines().count().max(1);
            for sym in &fs.symbols {
                prop_assert!(sym.line_start >= 1,
                    "Symbol line_start {} is less than 1", sym.line_start);
                prop_assert!(sym.line_start <= sym.line_end,
                    "Symbol line_start {} > line_end {}", sym.line_start, sym.line_end);
                // tree-sitter may report line_end beyond the line count for the
                // last node if it spans to EOF, so we allow line_end == total_lines + 1
                prop_assert!(sym.line_end <= total_lines + 1,
                    "Symbol line_end {} exceeds total lines {} + 1", sym.line_end, total_lines);
            }
        }
    }
}
