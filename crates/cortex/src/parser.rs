use std::path::Path;

use synapseed_core::error::{Result, SynapseedError};
use synapseed_core::symbol::{FileStructure, Symbol, SymbolId, SymbolKind};
use tracing::debug;

use crate::language::Language;

/// AST parser that extracts structured symbols from source files.
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
    pub fn parse_file(&mut self, path: &Path, source: &str) -> Result<FileStructure> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let lang = Language::from_extension(ext).ok_or_else(|| SynapseedError::Parse {
            file: path.display().to_string(),
            reason: format!("Unsupported file extension: .{ext}"),
        })?;

        let parser = self.parsers.get_mut(&lang).ok_or_else(|| {
            SynapseedError::Internal(format!("No parser for {lang}"))
        })?;

        let tree = parser.parse(source, None).ok_or_else(|| SynapseedError::Parse {
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
                "impl_item" | "struct_item" => Some(SymbolKind::Struct),
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
        };

        if let Some(sk) = symbol_kind {
            let name = Self::extract_name(node, source, lang).unwrap_or_else(|| "<anon>".into());
            let signature = Self::extract_signature(node, source);

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
