//! Diagnostic types — parsed from `cargo check --message-format=json`.

use serde::{Deserialize, Serialize};

/// A single compiler diagnostic with location and optional fix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub file_path: String,
    pub level: DiagnosticLevel,
    pub message: String,
    pub code: Option<String>,
    pub line_start: usize,
    pub line_end: usize,
    pub column_start: usize,
    pub column_end: usize,
    pub rendered: String,
    pub suggestions: Vec<Suggestion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticLevel {
    Error,
    Warning,
    Note,
    Help,
    Ice,
}

/// A suggested fix from the compiler (e.g., "remove this `mut`").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub message: String,
    pub file_path: String,
    pub line_start: usize,
    pub line_end: usize,
    pub column_start: usize,
    pub column_end: usize,
    pub replacement: String,
    pub applicability: Applicability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Applicability {
    MachineApplicable,
    MaybeIncorrect,
    HasPlaceholders,
    Unspecified,
}

/// Snapshot of all diagnostics at a point in time.
#[derive(Debug, Clone, Default, Serialize)]
pub struct DiagnosticSnapshot {
    pub diagnostics: Vec<Diagnostic>,
    pub error_count: usize,
    pub warning_count: usize,
    pub last_check_ms: u64,
}

// ── Cargo JSON Deserialization ──────────────────────────────────────

/// Top-level cargo check JSON line.
#[derive(Deserialize)]
pub(crate) struct CargoMessage {
    pub reason: String,
    pub message: Option<CompilerMessage>,
}

#[derive(Deserialize)]
pub(crate) struct CompilerMessage {
    pub message: String,
    pub code: Option<DiagnosticCode>,
    pub level: String,
    pub spans: Vec<SpanInfo>,
    pub children: Vec<CompilerMessage>,
    pub rendered: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct DiagnosticCode {
    pub code: String,
}

#[derive(Deserialize)]
pub(crate) struct SpanInfo {
    pub file_name: String,
    pub line_start: usize,
    pub line_end: usize,
    pub column_start: usize,
    pub column_end: usize,
    pub is_primary: bool,
    pub suggested_replacement: Option<String>,
    pub suggestion_applicability: Option<String>,
}

/// Parse a single cargo check JSON line into diagnostics.
pub(crate) fn parse_cargo_line(line: &str) -> Vec<Diagnostic> {
    let msg: CargoMessage = match serde_json::from_str(line) {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };

    if msg.reason != "compiler-message" {
        return Vec::new();
    }

    let compiler_msg = match msg.message {
        Some(m) => m,
        None => return Vec::new(),
    };

    let level = match compiler_msg.level.as_str() {
        "error" => DiagnosticLevel::Error,
        "warning" => DiagnosticLevel::Warning,
        "note" => DiagnosticLevel::Note,
        "help" => DiagnosticLevel::Help,
        _ => return Vec::new(),
    };

    // Find the primary span
    let primary_span = compiler_msg
        .spans
        .iter()
        .find(|s| s.is_primary)
        .or_else(|| compiler_msg.spans.first());

    let (file_path, line_start, line_end, col_start, col_end) = match primary_span {
        Some(span) => (
            span.file_name.clone(),
            span.line_start,
            span.line_end,
            span.column_start,
            span.column_end,
        ),
        None => return Vec::new(),
    };

    // Extract suggestions from children
    let mut suggestions = Vec::new();
    for child in &compiler_msg.children {
        for span in &child.spans {
            if let Some(ref replacement) = span.suggested_replacement {
                let applicability = match span.suggestion_applicability.as_deref().unwrap_or("") {
                    "MachineApplicable" => Applicability::MachineApplicable,
                    "MaybeIncorrect" => Applicability::MaybeIncorrect,
                    "HasPlaceholders" => Applicability::HasPlaceholders,
                    _ => Applicability::Unspecified,
                };

                suggestions.push(Suggestion {
                    message: child.message.clone(),
                    file_path: span.file_name.clone(),
                    line_start: span.line_start,
                    line_end: span.line_end,
                    column_start: span.column_start,
                    column_end: span.column_end,
                    replacement: replacement.clone(),
                    applicability,
                });
            }
        }
    }

    vec![Diagnostic {
        file_path,
        level,
        message: compiler_msg.message,
        code: compiler_msg.code.map(|c| c.code),
        line_start,
        line_end,
        column_start: col_start,
        column_end: col_end,
        rendered: compiler_msg.rendered.unwrap_or_default(),
        suggestions,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_error_diagnostic() {
        let json = r#"{"reason":"compiler-message","package_id":"test 0.1.0","manifest_path":"/test/Cargo.toml","target":{"kind":["lib"],"crate_types":["lib"],"name":"test","src_path":"/test/src/lib.rs","edition":"2021","doc":true,"doctest":true,"test":true},"message":{"rendered":"error[E0425]: cannot find value `x`","message":"cannot find value `x` in this scope","code":{"code":"E0425","explanation":null},"level":"error","spans":[{"file_name":"src/lib.rs","byte_start":100,"byte_end":101,"line_start":5,"line_end":5,"column_start":10,"column_end":11,"is_primary":true,"text":[{"text":"    let y = x;","highlight_start":13,"highlight_end":14}],"label":"not found in this scope","suggested_replacement":null,"suggestion_applicability":null,"expansion":null}],"children":[]}}"#;

        let diags = parse_cargo_line(json);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].level, DiagnosticLevel::Error);
        assert_eq!(diags[0].code.as_deref(), Some("E0425"));
        assert_eq!(diags[0].file_path, "src/lib.rs");
        assert_eq!(diags[0].line_start, 5);
    }

    #[test]
    fn test_parse_non_compiler_message() {
        let json = r#"{"reason":"build-finished","success":true}"#;
        let diags = parse_cargo_line(json);
        assert!(diags.is_empty());
    }

    #[test]
    fn test_parse_warning_with_suggestion() {
        let json = r#"{"reason":"compiler-message","package_id":"test 0.1.0","manifest_path":"/test/Cargo.toml","target":{"kind":["lib"],"crate_types":["lib"],"name":"test","src_path":"/test/src/lib.rs","edition":"2021","doc":true,"doctest":true,"test":true},"message":{"rendered":"warning: unused variable","message":"unused variable: `x`","code":{"code":"unused_variables","explanation":null},"level":"warning","spans":[{"file_name":"src/lib.rs","byte_start":50,"byte_end":51,"line_start":3,"line_end":3,"column_start":9,"column_end":10,"is_primary":true,"text":[],"label":null,"suggested_replacement":null,"suggestion_applicability":null,"expansion":null}],"children":[{"message":"if this is intentional, prefix it with an underscore","code":null,"level":"help","spans":[{"file_name":"src/lib.rs","byte_start":50,"byte_end":51,"line_start":3,"line_end":3,"column_start":9,"column_end":10,"is_primary":true,"text":[],"label":null,"suggested_replacement":"_x","suggestion_applicability":"MachineApplicable","expansion":null}],"children":[],"rendered":null}]}}"#;

        let diags = parse_cargo_line(json);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].level, DiagnosticLevel::Warning);
        assert_eq!(diags[0].suggestions.len(), 1);
        assert_eq!(diags[0].suggestions[0].replacement, "_x");
        assert_eq!(
            diags[0].suggestions[0].applicability,
            Applicability::MachineApplicable
        );
    }
}
