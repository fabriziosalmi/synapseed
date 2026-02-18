//! Query cleaning and synonym expansion for search.
//!
//! Transforms user queries (English/Italian) into boosted Tantivy query strings
//! with CamelCase/snake_case splitting and conservative synonym expansion.
//!
//! Split from extraction (#64).

use std::collections::HashSet;
use std::sync::LazyLock;

// ── Stop Words (v5.0.1: externalized to data/stop_words.txt, #72) ──────

/// Words to ignore when searching for symbols and when cleaning queries.
/// Using HashSet for O(1) lookup instead of O(n) array scan.
/// Loaded from `crates/whisper/data/stop_words.txt` at compile time.
pub(in crate::router) static STOP_WORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    include_str!("../../../data/stop_words.txt")
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
});

// ── Query Cleaning ─────────────────────────────────────────────────────

/// Strip stop words and return cleaned technical terms for Tantivy search.
/// "Come funziona il chunked transfer encoding in requests?" → "chunked transfer encoding requests"
///
/// CamelCase Splitting (v4.1.0): "MomentumEngine" → "MomentumEngine Momentum Engine momentum engine".
/// snake_case Splitting (v5.0.0): "build_smart_context" → "build_smart_context build smart context".
/// This maximizes BM25 signal for compound identifiers that `en_stem` treats as
/// a single opaque token ("momentumengin"). Both original-case and lowercase
/// components are emitted for unambiguous matching.
pub(in crate::router) fn clean_query_for_search(query: &str) -> String {
    let terms: Vec<&str> = query
        .split(|c: char| c.is_whitespace() || c == '?' || c == '!' || c == ',')
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '_'))
        .filter(|w| w.len() >= 2 && !STOP_WORDS.contains(&w.to_lowercase().as_str()))
        .collect();

    // CamelCase splitting (v4.1.0) + snake_case splitting (v5.0.0):
    // "MomentumEngine" → ["MomentumEngine", "Momentum", "Engine", "momentum", "engine"]
    // "build_smart_context" → ["build_smart_context", "build", "smart", "context"]
    let mut expanded = Vec::new();
    for term in &terms {
        expanded.push(term.to_string());

        // snake_case expansion (v5.0.0): split on underscores
        if term.contains('_') {
            for part in term.split('_') {
                if part.len() >= 3
                    && !expanded.iter().any(|e| e.eq_ignore_ascii_case(part))
                    && !STOP_WORDS.contains(&part.to_lowercase().as_str())
                {
                    expanded.push(part.to_string());
                }
            }
        }

        // CamelCase expansion: split compound identifiers
        for part in split_camel_case(term) {
            if part.len() >= 3 && !expanded.iter().any(|e| e.eq_ignore_ascii_case(&part)) {
                let lower = part.to_lowercase();
                expanded.push(part.clone());
                if lower != part {
                    expanded.push(lower);
                }
            }
        }
    }

    // Synonym expansion (v3.9.4 → v4.17.2: Conservative Boosting).
    // Original terms get ^3 boost, synonyms get ^0.5 to avoid BM25 signal dilution.
    // Max 2 synonyms per original term to prevent over-expansion (P1 fix).
    let base_count = expanded.len();
    let mut synonyms_for_query: Vec<String> = Vec::new();
    for i in 0..base_count {
        let lower = expanded[i].to_lowercase();
        let syns = expand_synonyms(&lower);
        let mut added = 0;
        for synonym in syns {
            if added >= 2 {
                break; // Cap at 2 synonyms per original term
            }
            if !expanded.iter().any(|e| e.to_lowercase() == synonym)
                && !synonyms_for_query.iter().any(|s| s == &synonym)
            {
                synonyms_for_query.push(synonym);
                added += 1;
            }
        }
    }

    // Build boosted Tantivy query: originals^3 + synonyms^0.5
    let mut boosted_parts: Vec<String> = Vec::new();
    for term in &expanded {
        // Escape special chars that Tantivy QueryParser interprets
        let clean = term.replace([':', '(', ')'], " ");
        let clean = clean.trim();
        if !clean.is_empty() {
            boosted_parts.push(format!("{clean}^3"));
        }
    }
    for syn in &synonyms_for_query {
        let clean = syn.replace([':', '(', ')'], " ");
        let clean = clean.trim();
        if !clean.is_empty() {
            boosted_parts.push(format!("{clean}^0.5"));
        }
    }

    boosted_parts.join(" ")
}

/// Split a CamelCase identifier into its constituent words.
///
/// "MomentumEngine" → ["Momentum", "Engine"]
/// "CodeGraph" → ["Code", "Graph"]
/// "BM25" → ["BM25"] (acronym+number stays together)
/// "build_smart_context" → [] (snake_case → no splits)
/// "URLParser" → ["URL", "Parser"]
fn split_camel_case(s: &str) -> Vec<String> {
    // Only split if the string contains mixed case (not all-upper, not all-lower, not snake_case)
    if s.contains('_')
        || s.chars().all(|c| c.is_uppercase() || !c.is_alphabetic())
        || s.chars().all(|c| c.is_lowercase() || !c.is_alphabetic())
    {
        return Vec::new();
    }

    let chars: Vec<char> = s.chars().collect();
    let mut parts = Vec::new();
    let mut start = 0;

    for i in 1..chars.len() {
        // Split at: lowercase/digit → Uppercase ("mE" in "MomentumEngine", "5S" in "BM25Score")
        // Split at: Uppercase → Uppercase+lowercase ("LP" → "L" | "Pa" in "URLParser")
        let prev_lower_or_digit = chars[i - 1].is_lowercase() || chars[i - 1].is_ascii_digit();
        let split_here = (chars[i].is_uppercase() && prev_lower_or_digit)
            || (i + 1 < chars.len()
                && chars[i].is_uppercase()
                && chars[i + 1].is_lowercase()
                && chars[i - 1].is_uppercase());

        if split_here {
            let part: String = chars[start..i].iter().collect();
            if part.len() >= 2 {
                parts.push(part);
            }
            start = i;
        }
    }

    // Last segment
    let part: String = chars[start..].iter().collect();
    if part.len() >= 2 {
        parts.push(part);
    }

    // Only return if we actually split into multiple parts
    if parts.len() > 1 {
        parts
    } else {
        Vec::new()
    }
}

/// Expand a technical term with morphological variants and domain synonyms.
/// Returns additional terms to append to the search query.
pub(in crate::router) fn expand_synonyms(term: &str) -> Vec<String> {
    let mut synonyms = Vec::new();

    // Morphological: strip common suffixes to get root form
    if let Some(root) = term.strip_suffix("ed") {
        synonyms.push(root.to_string()); // "chunked" → "chunk"
        synonyms.push(format!("{root}ing")); // "chunked" → "chunking"
    } else if let Some(root) = term.strip_suffix("ing") {
        synonyms.push(root.to_string()); // "encoding" → "encod"
        synonyms.push(format!("{root}e")); // "encoding" → "encode"
    } else if let Some(root) = term.strip_suffix("tion") {
        synonyms.push(root.to_string()); // "authentication" → "authentica"
        synonyms.push(format!("{root}te")); // "authentication" → "authenticate"
    }

    // Domain-specific synonyms (bidirectional, v5.0.0: expanded)
    static SYNONYM_PAIRS: &[(&str, &[&str])] = &[
        ("encode", &["decode", "codec", "encoding"]),
        ("decode", &["encode", "codec", "decoding"]),
        ("chunk", &["stream", "iter", "iterate"]),
        ("route", &["router", "routing", "dispatch"]),
        ("handle", &["handler", "middleware"]),
        ("auth", &["authenticate", "authorization", "login"]),
        ("request", &["response", "http"]),
        ("parse", &["parser", "parsing", "deserialize"]),
        ("serialize", &["deserialize", "marshal", "unmarshal"]),
        ("connect", &["connection", "socket", "transport"]),
        ("transfer", &["transport", "stream"]),
        // v5.0.0: new pairs for better recall
        ("index", &["search", "query"]),
        ("search", &["index", "find"]),
        ("test", &["spec", "assert", "expect"]),
        ("config", &["configuration", "settings", "options"]),
        ("build", &["compile", "make", "assemble"]),
        ("error", &["exception", "failure", "fault"]),
        ("cache", &["memoize", "store", "buffer"]),
        ("async", &["await", "future", "promise"]),
        ("graph", &["tree", "node", "edge"]),
        ("schema", &["model", "definition", "structure"]),
        ("context", &["ctx", "scope", "environment"]),
        ("symbol", &["token", "identifier", "name"]),
        ("inject", &["injection", "provide", "supply"]),
        ("extract", &["extraction", "parse", "pull"]),
        // v4.17.2 (P1 fix): trimmed to top-2 strongest synonyms per term.
        // Over-expansion causes BM25 signal dilution (mark_applied beating score_results).
        ("ranking", &["score", "rank"]),
        ("score", &["scoring", "ranking"]),
        ("weight", &["boost", "factor"]),
        ("boost", &["weight", "factor"]),
        ("plugin", &["extension", "module", "addon"]),
        ("sandbox", &["isolate", "isolation", "jail", "container"]),
        ("gym", &["sandbox", "evaluate", "train"]),
    ];

    for &(key, values) in SYNONYM_PAIRS {
        if term == key {
            for &v in values {
                if !synonyms.contains(&v.to_string()) {
                    synonyms.push(v.to_string());
                }
            }
        }
    }

    synonyms
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Query cleaning tests (v3.9.0) ─────────────────────────────

    #[test]
    fn test_clean_query_italian_stops_removed() {
        let cleaned =
            clean_query_for_search("Come funziona il chunked transfer encoding in requests?");
        // Core terms preserved with ^3 boost (v4.17.2 Conservative Boosting)
        assert!(cleaned.contains("chunked^3"));
        assert!(cleaned.contains("transfer^3"));
        assert!(cleaned.contains("encoding^3"));
        assert!(cleaned.contains("requests^3"));
        // Synonym expansion adds related terms with ^0.5 boost
        assert!(cleaned.contains("chunk^")); // from "chunked"
    }

    #[test]
    fn test_clean_query_italian_extended_stops() {
        // v3.9.4: "viene gestita decodifica funzioni chiamano riga" should all be removed
        let cleaned = clean_query_for_search(
            "In quale file e riga viene gestita la decodifica del chunked transfer encoding e quali funzioni lo chiamano?"
        );
        // Core technical terms preserved, synonyms expanded
        assert!(cleaned.contains("chunked"));
        assert!(cleaned.contains("transfer"));
        assert!(cleaned.contains("encoding"));
        // Synonym expansion: "chunked" → "chunk", "encoding" → "encode"
        assert!(cleaned.contains("chunk"));
        assert!(cleaned.contains("stream")); // synonym of chunk
                                             // Italian noise removed
        assert!(!cleaned.contains("viene"));
        assert!(!cleaned.contains("gestita"));
        assert!(!cleaned.contains("decodifica"));
        assert!(!cleaned.contains("riga"));
        assert!(!cleaned.contains("funzioni"));
        assert!(!cleaned.contains("chiamano"));
    }

    #[test]
    fn test_clean_query_english_stops_removed() {
        let cleaned =
            clean_query_for_search("How does the authentication flow work in this project?");
        // Core terms preserved with ^3 boost (v4.17.2)
        assert!(cleaned.contains("authentication^3"));
        assert!(cleaned.contains("flow^3"));
        assert!(cleaned.contains("project^3"));
        // "authentication" → synonym expansion via "tion" suffix stripping + auth synonyms
        assert!(cleaned.contains("authenticate^"));
    }

    #[test]
    fn test_clean_query_preserves_technical_terms() {
        let cleaned = clean_query_for_search("explain tokio::spawn and async runtime");
        // v4.17.2: colons get escaped to spaces, but terms are preserved with boost
        assert!(cleaned.contains("tokio"));
        assert!(cleaned.contains("spawn"));
        assert!(cleaned.contains("async^3"));
        assert!(cleaned.contains("runtime^3"));
    }

    #[test]
    fn test_synonym_expansion() {
        // "chunked" → "chunk", "chunking"
        let synonyms = expand_synonyms("chunked");
        assert!(synonyms.contains(&"chunk".to_string()));

        // "encoding" → "encode" via morphological strip
        let synonyms = expand_synonyms("encoding");
        assert!(synonyms.contains(&"encode".to_string()));

        // "chunk" → domain synonyms "stream", "iter"
        let synonyms = expand_synonyms("chunk");
        assert!(synonyms.contains(&"stream".to_string()));
        assert!(synonyms.contains(&"iter".to_string()));

        // Unknown word → no synonyms
        let synonyms = expand_synonyms("foobar");
        assert!(synonyms.is_empty());
    }

    #[test]
    fn test_clean_query_empty_result() {
        let cleaned = clean_query_for_search("come il la un");
        assert!(cleaned.is_empty());
    }

    // ── CamelCase splitting tests (v4.1.0) ──────────────────────────

    #[test]
    fn test_split_camel_case_basic() {
        assert_eq!(
            split_camel_case("MomentumEngine"),
            vec!["Momentum", "Engine"]
        );
        assert_eq!(split_camel_case("CodeGraph"), vec!["Code", "Graph"]);
        assert_eq!(
            split_camel_case("SmartContextInput"),
            vec!["Smart", "Context", "Input"]
        );
    }

    #[test]
    fn test_split_camel_case_acronym() {
        assert_eq!(split_camel_case("URLParser"), vec!["URL", "Parser"]);
        assert_eq!(split_camel_case("BM25Score"), vec!["BM25", "Score"]);
    }

    #[test]
    fn test_split_camel_case_no_split() {
        // snake_case → no split
        assert!(split_camel_case("build_smart_context").is_empty());
        // all lowercase → no split
        assert!(split_camel_case("tokio").is_empty());
        // all uppercase → no split
        assert!(split_camel_case("HTML").is_empty());
        // too short parts
        assert!(split_camel_case("aB").is_empty());
    }

    #[test]
    fn test_clean_query_camelcase_expansion() {
        let cleaned = clean_query_for_search("explain the MomentumEngine tier system");
        assert!(cleaned.contains("MomentumEngine"));
        assert!(cleaned.contains("Momentum"));
        assert!(cleaned.contains("Engine"));
        assert!(cleaned.contains("momentum"));
        assert!(cleaned.contains("engine"));
        assert!(cleaned.contains("tier"));
        assert!(cleaned.contains("system"));
    }
}
