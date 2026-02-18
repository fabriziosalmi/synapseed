//! String extraction and classification from binary data.

use serde::Serialize;

/// A string extracted from a binary, with classification.
#[derive(Debug, Clone, Serialize)]
pub struct ClassifiedString {
    /// The raw string value.
    pub value: String,
    /// Offset in the binary.
    pub offset: usize,
    /// Classification.
    pub class: StringClass,
}

/// String classification based on content patterns.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum StringClass {
    /// URL or URI (http://, https://, ftp://, ws://).
    Url,
    /// File path (/usr/..., C:\..., ./...).
    FilePath,
    /// Error message (contains "error", "failed", "panic", etc.).
    ErrorMessage,
    /// Environment variable reference ($VAR, %VAR%).
    EnvVar,
    /// SQL query (SELECT, INSERT, CREATE, etc.).
    SqlQuery,
    /// Format string (contains %s, %d, {}, etc.).
    FormatString,
    /// Version string (semver-like).
    Version,
    /// Crate/package name (from Cargo/npm patterns).
    PackageName,
    /// Log message (DEBUG, INFO, WARN, ERROR prefix).
    LogMessage,
    /// General text (no specific classification).
    General,
}

/// Extract printable ASCII strings from binary data.
///
/// `min_length` is the minimum string length to extract (recommended: 4-6).
pub fn extract_strings(data: &[u8], min_length: usize) -> Vec<(usize, String)> {
    let mut strings = Vec::new();
    let mut current = String::new();
    let mut start = 0;

    for (i, &byte) in data.iter().enumerate() {
        if (0x20..0x7F).contains(&byte) {
            if current.is_empty() {
                start = i;
            }
            current.push(byte as char);
        } else {
            if current.len() >= min_length {
                strings.push((start, current.clone()));
            }
            current.clear();
        }
    }

    // Don't forget trailing string
    if current.len() >= min_length {
        strings.push((start, current));
    }

    strings
}

/// Classify extracted strings.
pub fn classify_strings(raw: &[(usize, String)]) -> Vec<ClassifiedString> {
    raw.iter()
        .map(|(offset, value)| ClassifiedString {
            class: classify_one(value),
            value: value.clone(),
            offset: *offset,
        })
        .collect()
}

/// Classify a single string.
fn classify_one(s: &str) -> StringClass {
    let lower = s.to_lowercase();

    // URL
    if s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("ftp://")
        || s.starts_with("ws://")
        || s.starts_with("wss://")
    {
        return StringClass::Url;
    }

    // File path
    if s.starts_with('/')
        || s.starts_with("./")
        || s.starts_with("../")
        || (s.len() >= 3 && s.as_bytes()[1] == b':' && s.as_bytes()[2] == b'\\')
    {
        return StringClass::FilePath;
    }

    // SQL
    let sql_keywords = [
        "select ",
        "insert ",
        "update ",
        "delete ",
        "create table",
        "alter table",
        "drop ",
    ];
    if sql_keywords.iter().any(|kw| lower.starts_with(kw)) {
        return StringClass::SqlQuery;
    }

    // Environment variable
    if (s.starts_with('$')
        && s.len() > 1
        && s[1..].chars().all(|c| c.is_alphanumeric() || c == '_'))
        || (s.starts_with('%') && s.ends_with('%') && s.len() > 2)
    {
        return StringClass::EnvVar;
    }

    // Version string
    if is_version_string(s) {
        return StringClass::Version;
    }

    // Log message
    if lower.starts_with("debug:")
        || lower.starts_with("info:")
        || lower.starts_with("warn:")
        || lower.starts_with("error:")
        || lower.starts_with("trace:")
    {
        return StringClass::LogMessage;
    }

    // Error message
    let error_markers = [
        "error",
        "failed",
        "panic",
        "fatal",
        "abort",
        "segfault",
        "exception",
        "invalid",
        "cannot",
        "unable to",
        "unexpected",
    ];
    if error_markers.iter().any(|m| lower.contains(m)) {
        return StringClass::ErrorMessage;
    }

    // Format string
    if s.contains("%s")
        || s.contains("%d")
        || s.contains("%f")
        || s.contains("%x")
        || s.contains("{}")
        || s.contains("{:?}")
        || s.contains("{:#?}")
    {
        return StringClass::FormatString;
    }

    // Package name (crate-like: lowercase with hyphens)
    if s.len() <= 64
        && !s.contains(' ')
        && s.contains('-')
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c == '-' || c.is_ascii_digit())
    {
        return StringClass::PackageName;
    }

    StringClass::General
}

/// Check if a string looks like a version (semver-ish).
fn is_version_string(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() < 2 || parts.len() > 4 {
        return false;
    }
    // First part might have a 'v' prefix
    let first = parts[0].strip_prefix('v').unwrap_or(parts[0]);
    first.parse::<u32>().is_ok() && parts[1].chars().take(3).all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_strings() {
        let data = b"hello\x00world\x00hi\x00long_string_here\x00";
        let strings = extract_strings(data, 4);
        assert_eq!(strings.len(), 3);
        assert_eq!(strings[0].1, "hello");
        assert_eq!(strings[1].1, "world");
        assert_eq!(strings[2].1, "long_string_here");
    }

    #[test]
    fn test_classify_url() {
        assert_eq!(classify_one("https://example.com"), StringClass::Url);
    }

    #[test]
    fn test_classify_path() {
        assert_eq!(classify_one("/usr/bin/synapseed"), StringClass::FilePath);
    }

    #[test]
    fn test_classify_error() {
        assert_eq!(
            classify_one("connection failed: timeout"),
            StringClass::ErrorMessage
        );
    }

    #[test]
    fn test_classify_sql() {
        assert_eq!(classify_one("SELECT * FROM users"), StringClass::SqlQuery);
    }

    #[test]
    fn test_classify_version() {
        assert_eq!(classify_one("4.15.0"), StringClass::Version);
        assert_eq!(classify_one("v1.2.3"), StringClass::Version);
    }

    #[test]
    fn test_classify_format() {
        assert_eq!(
            classify_one("value = {} at line {}"),
            StringClass::FormatString
        );
    }

    #[test]
    fn test_classify_general() {
        assert_eq!(classify_one("some regular text here"), StringClass::General);
    }
}
