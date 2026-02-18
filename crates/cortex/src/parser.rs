use std::path::Path;

use synapseed_core::error::{Result, SynapseedError};
use synapseed_core::symbol::{FileStructure, Symbol, SymbolId, SymbolKind, Visibility};
use tracing::debug;

use crate::language::Language;

/// AST parser that extracts structured symbols from source files.
/// Falls back to text-only extraction for unsupported languages.
pub struct AstParser {
    parsers: std::collections::HashMap<Language, tree_sitter::Parser>,
}

impl AstParser {
    pub fn new() -> Result<Self> {
        let mut parsers = std::collections::HashMap::new();

        for lang in [
            Language::Rust,
            Language::Python,
            Language::JavaScript,
            Language::TypeScript,
        ] {
            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&lang.ts_language()?)
                .map_err(|e| SynapseedError::Internal(format!("Failed to set language: {e}")))?;
            parsers.insert(lang, parser);
        }

        Ok(Self { parsers })
    }

    /// Parse a file and extract its structural skeleton (symbols only, no bodies).
    ///
    /// For languages with tree-sitter support, performs full AST extraction.
    /// For `Language::Unknown`, falls back to text-only extraction (line count,
    /// TODO/FIXME markers) so the file still appears in the code graph.
    pub fn parse_file(&mut self, path: &Path, source: &str) -> Result<FileStructure> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        let lang = Language::from_extension(ext).ok_or_else(|| SynapseedError::Parse {
            file: path.display().to_string(),
            reason: format!("Unsupported file extension: .{ext}"),
        })?;

        // Text-only fallback for languages without tree-sitter grammars
        if !lang.has_ast_support() {
            return Ok(Self::text_only_parse(path, source, lang));
        }

        let parser = self
            .parsers
            .get_mut(&lang)
            .ok_or_else(|| SynapseedError::Internal(format!("No parser for {lang}")))?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| SynapseedError::Parse {
                file: path.display().to_string(),
                reason: "Tree-sitter parse returned None".into(),
            })?;

        let mut symbols = Vec::new();
        Self::extract_symbols(tree.root_node(), source, &mut symbols, lang, None);

        debug!(
            path = %path.display(),
            lang = %lang,
            symbols = symbols.len(),
            "Parsed file structure"
        );

        Ok(FileStructure {
            path: path.display().to_string(),
            language: lang.name().to_string(),
            symbols,
        })
    }

    /// Text-only fallback: extract line count and TODO/FIXME markers as symbols.
    /// This ensures that files in unsupported languages still appear in the code graph,
    /// the architect module metrics, and the search index.
    fn text_only_parse(path: &Path, source: &str, lang: Language) -> FileStructure {
        let mut symbols = Vec::new();
        let total_lines = source.lines().count();

        // Extract TODO/FIXME/HACK/XXX markers as symbols
        for (line_idx, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            let marker = if let Some(pos) = trimmed.find("TODO") {
                Some(("TODO", pos, trimmed))
            } else if let Some(pos) = trimmed.find("FIXME") {
                Some(("FIXME", pos, trimmed))
            } else if let Some(pos) = trimmed.find("HACK") {
                Some(("HACK", pos, trimmed))
            } else {
                trimmed.find("XXX").map(|pos| ("XXX", pos, trimmed))
            };

            if let Some((tag, _, line_text)) = marker {
                // Extract the comment text after the marker
                let signature = line_text.to_string();
                symbols.push(Symbol {
                    id: SymbolId::new(),
                    name: format!(
                        "{tag}:{}",
                        path.file_name().and_then(|n| n.to_str()).unwrap_or("?")
                    ),
                    kind: SymbolKind::Variable, // reuse Variable kind for markers
                    file_path: String::new(),
                    line_start: line_idx + 1,
                    line_end: line_idx + 1,
                    signature: Some(signature),
                    visibility: None,
                    children: Vec::new(),
                });
            }
        }

        debug!(
            path = %path.display(),
            lang = %lang,
            lines = total_lines,
            markers = symbols.len(),
            "Text-only parse (no AST)"
        );

        FileStructure {
            path: path.display().to_string(),
            language: lang.name().to_string(),
            symbols,
        }
    }

    /// Extract symbols from a tree-sitter node.
    ///
    /// `impl_context` carries the parent impl block's (type_name, trait_name)
    /// so that methods inside a trait impl get Fully Qualified Path names (D61).
    fn extract_symbols(
        node: tree_sitter::Node,
        source: &str,
        symbols: &mut Vec<Symbol>,
        lang: Language,
        impl_context: Option<(&str, Option<&str>)>,
    ) {
        let kind = node.kind();

        let symbol_kind = match lang {
            Language::Rust => match kind {
                "function_item" => Some(SymbolKind::Function),
                "struct_item" => Some(SymbolKind::Struct),
                "trait_item" => Some(SymbolKind::Interface),
                "impl_item" => Some(SymbolKind::Struct), // name extracted via "type" field below
                "enum_item" => Some(SymbolKind::Enum),
                "mod_item" => Some(SymbolKind::Module),
                "const_item" | "static_item" => Some(SymbolKind::Constant),
                "use_declaration" => Some(SymbolKind::Import),
                "macro_invocation" | "macro_definition" | "macro_rules_definition" => {
                    Some(SymbolKind::Macro)
                }
                _ => None,
            },
            Language::Python => match kind {
                "function_definition" => Some(SymbolKind::Function),
                "class_definition" => Some(SymbolKind::Class),
                "import_statement" | "import_from_statement" => Some(SymbolKind::Import),
                _ => None,
            },
            Language::JavaScript | Language::TypeScript => match kind {
                "function_declaration" | "arrow_function" => Some(SymbolKind::Function),
                "class_declaration" => Some(SymbolKind::Class),
                "import_statement" => Some(SymbolKind::Import),
                "lexical_declaration" => Some(SymbolKind::Variable),
                // TypeScript-specific via JS grammar parse:
                "interface_declaration" => Some(SymbolKind::Interface),
                "type_alias_declaration" => Some(SymbolKind::Struct),
                "enum_declaration" => Some(SymbolKind::Enum),
                _ => None,
            },
            Language::Unknown => None, // handled by text_only_parse
        };

        if let Some(sk) = symbol_kind {
            // Trait Expansion (v4.2.0): impl_item uses "type" field for the name,
            // and "trait" field for the trait reference (appended to signature for BM25).
            let (name, signature) = if kind == "impl_item" {
                let type_name = node
                    .child_by_field_name("type")
                    .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                    .map(String::from)
                    .unwrap_or_else(|| "<anon>".into());
                let trait_name = node
                    .child_by_field_name("trait")
                    .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                    .map(String::from);
                let sig = Self::extract_signature(node, source);
                // Enrich signature with "impl TRAIT for TYPE" for BM25 discoverability
                let enriched_sig = if let Some(ref tr) = trait_name {
                    Some(format!("{} [trait: {}]", sig.as_deref().unwrap_or(""), tr))
                } else {
                    sig
                };
                (type_name, enriched_sig)
            } else if kind == "class_definition" {
                // Inheritance Boost (v4.4.0): extract superclass names from Python
                // class definitions and append to signature for BM25 discoverability.
                // "class SecurityMiddleware(MiddlewareMixin):" → [inherits: MiddlewareMixin]
                let name =
                    Self::extract_name(node, source, lang).unwrap_or_else(|| "<anon>".into());
                let sig = Self::extract_signature(node, source);
                let parents = Self::extract_superclasses(node, source);
                let enriched_sig = if !parents.is_empty() {
                    Some(format!(
                        "{} [inherits: {}]",
                        sig.as_deref().unwrap_or(""),
                        parents.join(", ")
                    ))
                } else {
                    sig
                };
                (name, enriched_sig)
            } else {
                let raw_name =
                    Self::extract_name(node, source, lang).unwrap_or_else(|| "<anon>".into());
                // D61: Fully Qualified Path for methods inside trait impl blocks.
                // If this function_item lives inside `impl Trait for Type`, emit
                // `<Type as Trait>::method` so the LLM can disambiguate same-name methods.
                let name = if kind == "function_item" {
                    if let Some((type_name, Some(trait_name))) = impl_context {
                        format!("<{type_name} as {trait_name}>::{raw_name}")
                    } else if let Some((type_name, None)) = impl_context {
                        format!("{type_name}::{raw_name}")
                    } else {
                        raw_name
                    }
                } else {
                    raw_name
                };
                let signature = Self::extract_signature(node, source);
                (name, signature)
            };

            let visibility = Self::extract_visibility(node, source, lang);

            symbols.push(Symbol {
                id: SymbolId::new(),
                name,
                kind: sk,
                file_path: String::new(), // filled by caller
                line_start: node.start_position().row + 1,
                line_end: node.end_position().row + 1,
                signature,
                visibility,
                children: Vec::new(),
            });
        }

        // Recurse into children — propagate impl context for D61 FQP synthesis.
        let child_ctx: Option<(&str, Option<&str>)> = if kind == "impl_item" {
            // Extract type/trait names from this impl block for children.
            let type_name = node
                .child_by_field_name("type")
                .and_then(|n| n.utf8_text(source.as_bytes()).ok());
            let trait_name = node
                .child_by_field_name("trait")
                .and_then(|n| n.utf8_text(source.as_bytes()).ok());
            type_name.map(|t| (t, trait_name))
        } else {
            impl_context
        };

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            Self::extract_symbols(child, source, symbols, lang, child_ctx);
        }
    }

    fn extract_name(node: tree_sitter::Node, source: &str, lang: Language) -> Option<String> {
        let name_field = match lang {
            Language::Rust | Language::Python | Language::JavaScript | Language::TypeScript => {
                "name"
            }
            Language::Unknown => return None,
        };

        node.child_by_field_name(name_field)
            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
            .map(String::from)
    }

    /// Extract superclass names from a Python `class_definition` node.
    ///
    /// Handles both simple names (`class Foo(Bar)`) and dotted names
    /// (`class Foo(module.Bar)` → extracts `Bar`).
    fn extract_superclasses(node: tree_sitter::Node, source: &str) -> Vec<String> {
        let superclasses = match node.child_by_field_name("superclasses") {
            Some(sc) => sc,
            None => return Vec::new(),
        };
        let mut parents = Vec::new();
        let mut cursor = superclasses.walk();
        for child in superclasses.children(&mut cursor) {
            match child.kind() {
                "identifier" => {
                    if let Ok(text) = child.utf8_text(source.as_bytes()) {
                        parents.push(text.to_string());
                    }
                }
                "attribute" => {
                    // dotted name like `module.ClassName` → extract last component
                    if let Some(attr) = child.child_by_field_name("attribute") {
                        if let Ok(text) = attr.utf8_text(source.as_bytes()) {
                            parents.push(text.to_string());
                        }
                    }
                }
                _ => {}
            }
        }
        parents
    }

    /// Extract visibility modifier from a tree-sitter node (v4.9.0).
    ///
    /// Rust: `pub`, `pub(crate)`, `pub(super)` via `visibility_modifier` child.
    /// Python: names starting with `_` are private by convention.
    /// JavaScript: `export` keyword means public.
    fn extract_visibility(
        node: tree_sitter::Node,
        source: &str,
        lang: Language,
    ) -> Option<Visibility> {
        match lang {
            Language::Rust => {
                // tree-sitter-rust exposes "visibility_modifier" as a named child
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "visibility_modifier" {
                        let text = child.utf8_text(source.as_bytes()).ok()?;
                        return Some(if text.contains("crate") {
                            Visibility::Crate
                        } else if text.contains("super") {
                            Visibility::Super
                        } else {
                            // "pub" without restriction
                            Visibility::Public
                        });
                    }
                }
                // No visibility modifier → private
                Some(Visibility::Private)
            }
            Language::Python => {
                // Convention: _name = private, __name = very private
                let name = Self::extract_name(node, source, lang)?;
                Some(if name.starts_with('_') {
                    Visibility::Private
                } else {
                    Visibility::Public
                })
            }
            Language::JavaScript | Language::TypeScript => {
                // If the parent is an export_statement, it's public
                if let Some(parent) = node.parent() {
                    if parent.kind() == "export_statement" {
                        return Some(Visibility::Public);
                    }
                }
                Some(Visibility::Private)
            }
            Language::Unknown => None,
        }
    }

    fn extract_signature(node: tree_sitter::Node, source: &str) -> Option<String> {
        // D65: Multi-line signature extraction — collect all lines up to (and
        // excluding) the opening `{`, so `where T: Clone + Send` clauses are
        // captured instead of being truncated at the first line.
        let text = node.utf8_text(source.as_bytes()).ok()?;
        let mut sig_lines = Vec::new();
        for line in text.lines() {
            let trimmed = line.trim();
            // Stop BEFORE the opening brace (body start).
            if trimmed == "{" {
                break;
            }
            // If the line ends with '{', strip it and include the rest.
            if let Some(before_brace) = trimmed.strip_suffix('{') {
                let cleaned = before_brace.trim_end();
                if !cleaned.is_empty() {
                    sig_lines.push(cleaned.to_string());
                }
                break;
            }
            sig_lines.push(trimmed.to_string());
        }
        if sig_lines.is_empty() {
            return None;
        }
        // Join multi-line signatures with a single space for compact display.
        Some(sig_lines.join(" "))
    }
}
