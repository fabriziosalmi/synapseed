//! Code-aware tokenizer for Tantivy (v4.11.0 — "Il Linguista").
//!
//! Splits identifiers on CamelCase and snake_case boundaries.
//! No stemming — in code, precision > recall ("Factory" != "Factor").
//!
//! Tokenizer chain: word_split → camelCase_split → snake_case_split → lowercase.
//! Registered as `"code"` on the Tantivy Index, used for `symbol_name` and `signature`.

use tantivy::tokenizer::{Token, TokenStream, Tokenizer};

/// Code-aware tokenizer: splits CamelCase and snake_case, lowercases, no stemming.
///
/// `build_context`    → ["build_context", "build", "context"]
/// `MomentumEngine`   → ["momentumengine", "momentum", "engine"]
/// `_get_response`    → ["_get_response", "get", "response"]
/// `HTTPServer`       → ["httpserver", "http", "server"]
#[derive(Clone, Default)]
pub struct CodeTokenizer;

/// Pre-built token stream for the code tokenizer.
pub struct CodeTokenStream {
    tokens: Vec<Token>,
    index: usize,
}

impl Tokenizer for CodeTokenizer {
    type TokenStream<'a> = CodeTokenStream;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> CodeTokenStream {
        let mut tokens = Vec::new();
        let mut position = 0;

        // Split text into "code words" (alphanumeric + underscore runs)
        let mut start = None;
        for (i, c) in text.char_indices() {
            if c.is_alphanumeric() || c == '_' {
                if start.is_none() {
                    start = Some(i);
                }
            } else if let Some(s) = start.take() {
                emit_code_tokens(&text[s..i], s, i, &mut tokens, &mut position);
            }
        }
        if let Some(s) = start {
            emit_code_tokens(
                &text[s..text.len()],
                s,
                text.len(),
                &mut tokens,
                &mut position,
            );
        }

        CodeTokenStream { tokens, index: 0 }
    }
}

/// Emit tokens for a single code word, expanding CamelCase and snake_case.
fn emit_code_tokens(
    word: &str,
    offset_from: usize,
    offset_to: usize,
    tokens: &mut Vec<Token>,
    position: &mut usize,
) {
    if word.is_empty() {
        return;
    }

    let full_lower = word.to_lowercase();

    // Always emit the full identifier (lowercased)
    tokens.push(Token {
        offset_from,
        offset_to,
        position: *position,
        text: full_lower.clone(),
        position_length: 1,
    });
    *position += 1;

    // Snake_case split: "build_context" → ["build", "context"]
    if word.contains('_') {
        for part in word.split('_') {
            if part.len() >= 2 {
                let lower = part.to_lowercase();
                if lower != full_lower {
                    tokens.push(Token {
                        offset_from,
                        offset_to,
                        position: *position,
                        text: lower,
                        position_length: 1,
                    });
                    *position += 1;
                }
            }
        }
        return; // Don't also CamelCase-split snake_case identifiers
    }

    // CamelCase split: "MomentumEngine" → ["momentum", "engine"]
    let parts = split_camel_case(word);
    if parts.len() > 1 {
        for part in &parts {
            let lower = part.to_lowercase();
            if lower != full_lower {
                tokens.push(Token {
                    offset_from,
                    offset_to,
                    position: *position,
                    text: lower,
                    position_length: 1,
                });
                *position += 1;
            }
        }
    }
}

/// Split a CamelCase identifier into parts.
///
/// "MomentumEngine" → ["Momentum", "Engine"]
/// "HTTPServer"     → ["HTTP", "Server"]
/// "build"          → [] (single word, no split)
///
/// D38: Uses `char_indices()` instead of raw byte indexing to correctly
/// handle multibyte UTF-8 characters in Unicode identifiers.
fn split_camel_case(name: &str) -> Vec<&str> {
    let chars: Vec<(usize, char)> = name.char_indices().collect();
    if chars.len() < 2 {
        return Vec::new();
    }

    let mut parts = Vec::new();
    let mut start = 0; // byte offset

    for i in 1..chars.len() {
        let (byte_pos, cur) = chars[i];
        let (_, prev) = chars[i - 1];

        // Split at: lowercase → Uppercase ("aB")
        let lower_to_upper = prev.is_lowercase() && cur.is_uppercase();
        // Split at: Uppercase → Uppercase+lowercase ("ABc" → split before 'B')
        let upper_run_end = i + 1 < chars.len()
            && prev.is_uppercase()
            && cur.is_uppercase()
            && chars[i + 1].1.is_lowercase();

        if lower_to_upper || upper_run_end {
            let part = &name[start..byte_pos];
            if part.len() >= 2 {
                parts.push(part);
            }
            start = byte_pos;
        }
    }

    let part = &name[start..];
    if part.len() >= 2 {
        parts.push(part);
    }

    if parts.len() <= 1 {
        Vec::new()
    } else {
        parts
    }
}

/// Register the `"code"` tokenizer on a Tantivy Index.
///
/// Must be called after Index creation and before any indexing or searching.
pub fn register_code_tokenizer(index: &tantivy::Index) {
    use tantivy::tokenizer::TextAnalyzer;
    index
        .tokenizers()
        .register("code", TextAnalyzer::from(CodeTokenizer));
}

impl TokenStream for CodeTokenStream {
    fn advance(&mut self) -> bool {
        if self.index < self.tokens.len() {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn token(&self) -> &Token {
        &self.tokens[self.index - 1]
    }

    fn token_mut(&mut self) -> &mut Token {
        &mut self.tokens[self.index - 1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenize(text: &str) -> Vec<String> {
        let mut tokenizer = CodeTokenizer;
        let mut stream = tokenizer.token_stream(text);
        let mut tokens = Vec::new();
        while stream.advance() {
            tokens.push(stream.token().text.clone());
        }
        tokens
    }

    #[test]
    fn test_snake_case_split() {
        let tokens = tokenize("build_context");
        assert!(tokens.contains(&"build_context".to_string()));
        assert!(tokens.contains(&"build".to_string()));
        assert!(tokens.contains(&"context".to_string()));
    }

    #[test]
    fn test_camel_case_split() {
        let tokens = tokenize("MomentumEngine");
        assert!(tokens.contains(&"momentumengine".to_string()));
        assert!(tokens.contains(&"momentum".to_string()));
        assert!(tokens.contains(&"engine".to_string()));
    }

    #[test]
    fn test_http_acronym_split() {
        let tokens = tokenize("HTTPServer");
        assert!(tokens.contains(&"httpserver".to_string()));
        assert!(tokens.contains(&"http".to_string()));
        assert!(tokens.contains(&"server".to_string()));
    }

    #[test]
    fn test_underscore_prefix() {
        let tokens = tokenize("_get_response");
        assert!(tokens.contains(&"get".to_string()));
        assert!(tokens.contains(&"response".to_string()));
    }

    #[test]
    fn test_simple_word() {
        let tokens = tokenize("tokio");
        assert_eq!(tokens, vec!["tokio"]);
    }

    #[test]
    fn test_multiple_words() {
        let tokens = tokenize("authenticate_user MomentumEngine");
        assert!(tokens.contains(&"authenticate_user".to_string()));
        assert!(tokens.contains(&"authenticate".to_string()));
        assert!(tokens.contains(&"user".to_string()));
        assert!(tokens.contains(&"momentumengine".to_string()));
        assert!(tokens.contains(&"momentum".to_string()));
        assert!(tokens.contains(&"engine".to_string()));
    }

    #[test]
    fn test_no_stemming() {
        // "Factory" should NOT become "factori" (en_stem would do that)
        let tokens = tokenize("RequestFactory");
        assert!(tokens.contains(&"factory".to_string()));
        assert!(!tokens.iter().any(|t| t == "factori"));
    }

    #[test]
    fn test_unicode_identifiers_no_panic() {
        // D38: UTF-8 multibyte chars must not cause panic or incorrect splits
        let tokens = tokenize("Café");
        assert!(tokens.contains(&"café".to_string()));

        let tokens = tokenize("Straße");
        assert!(tokens.contains(&"straße".to_string()));

        // Mixed ASCII CamelCase + Unicode
        let tokens = tokenize("MyStörung");
        assert!(tokens.contains(&"mystörung".to_string()));

        // Pure ASCII still works
        let tokens = tokenize("CamelCase");
        assert!(tokens.contains(&"camelcase".to_string()));
        assert!(tokens.contains(&"camel".to_string()));
        assert!(tokens.contains(&"case".to_string()));
    }

    #[test]
    fn test_query_matches_index() {
        // Same tokenizer on both sides: query "Momentum" matches index "MomentumEngine"
        let index_tokens = tokenize("MomentumEngine");
        let query_tokens = tokenize("Momentum");
        // "momentum" appears in both
        assert!(query_tokens.contains(&"momentum".to_string()));
        assert!(index_tokens.contains(&"momentum".to_string()));
    }
}
