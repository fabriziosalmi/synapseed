use std::path::Path;

use synapseed_core::error::{Result, SynapseedError};
use synapseed_core::symbol::{FileStructure, Symbol, SymbolId, SymbolKind};
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

        for lang in [Language::Rust, Language::Python, Language::JavaScript] {
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
        Self::extract_symbols(tree.root_node(), source, &mut symbols, lang);

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
            } else { trimmed.find("XXX").map(|pos| ("XXX", pos, trimmed)) };

            if let Some((tag, _, line_text)) = marker {
                // Extract the comment text after the marker
                let signature = line_text.to_string();
                symbols.push(Symbol {
                    id: SymbolId::new(),
                    name: format!("{tag}:{}", path.file_name().and_then(|n| n.to_str()).unwrap_or("?")),
                    kind: SymbolKind::Variable, // reuse Variable kind for markers
                    file_path: String::new(),
                    line_start: line_idx + 1,
                    line_end: line_idx + 1,
                    signature: Some(signature),
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

    fn extract_symbols(
        node: tree_sitter::Node,
        source: &str,
        symbols: &mut Vec<Symbol>,
        lang: Language,
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
                _ => None,
            },
            Language::Python => match kind {
                "function_definition" => Some(SymbolKind::Function),
                "class_definition" => Some(SymbolKind::Class),
                "import_statement" | "import_from_statement" => Some(SymbolKind::Import),
                _ => None,
            },
            Language::JavaScript => match kind {
                "function_declaration" | "arrow_function" => Some(SymbolKind::Function),
                "class_declaration" => Some(SymbolKind::Class),
                "import_statement" => Some(SymbolKind::Import),
                "lexical_declaration" => Some(SymbolKind::Variable),
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
            } else {
                let name =
                    Self::extract_name(node, source, lang).unwrap_or_else(|| "<anon>".into());
                let signature = Self::extract_signature(node, source);
                (name, signature)
            };

            symbols.push(Symbol {
                id: SymbolId::new(),
                name,
                kind: sk,
                file_path: String::new(), // filled by caller
                line_start: node.start_position().row + 1,
                line_end: node.end_position().row + 1,
                signature,
                children: Vec::new(),
            });
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            Self::extract_symbols(child, source, symbols, lang);
        }
    }

    fn extract_name(node: tree_sitter::Node, source: &str, lang: Language) -> Option<String> {
        let name_field = match lang {
            Language::Rust | Language::Python | Language::JavaScript => "name",
            Language::Unknown => return None,
        };

        node.child_by_field_name(name_field)
            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
            .map(String::from)
    }

    fn extract_signature(node: tree_sitter::Node, source: &str) -> Option<String> {
        // Extract the first line (signature) without the body
        let text = node.utf8_text(source.as_bytes()).ok()?;
        let first_line = text.lines().next()?;
        Some(first_line.trim().to_string())
    }
}
