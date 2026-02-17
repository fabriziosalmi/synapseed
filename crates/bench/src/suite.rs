//! JSONL question suite loader.
//!
//! Each line in a `.jsonl` suite file is a JSON object:
//! ```json
//! {
//!   "id": "q01",
//!   "question": "What is the workspace version in Cargo.toml?",
//!   "ground_truth": ["4.15.0"],
//!   "expected_files": ["Cargo.toml"],
//!   "expected_symbols": [],
//!   "difficulty": "easy",
//!   "category": "factual"
//! }
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Difficulty tier for a benchmark question.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

/// Category of the question.
pub type QuestionCategory = String;

/// A single benchmark question loaded from JSONL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchQuestion {
    /// Unique identifier (e.g. "q01").
    pub id: String,

    /// The question to ask via `ask_raw`.
    pub question: String,

    /// Ground truth facts/patterns. Each entry is substring-matched
    /// against the `smart_context` response (case-insensitive).
    pub ground_truth: Vec<String>,

    /// Files that should appear in the response targets.
    #[serde(default)]
    pub expected_files: Vec<String>,

    /// Symbols that should appear in the response targets.
    #[serde(default)]
    pub expected_symbols: Vec<String>,

    /// Difficulty tier.
    pub difficulty: Difficulty,

    /// Question category (e.g. "factual", "structural", "behavioral").
    #[serde(default = "default_category")]
    pub category: QuestionCategory,
}

fn default_category() -> QuestionCategory {
    "general".into()
}

/// Load a JSONL question suite from disk.
///
/// Lines starting with `#` or empty lines are skipped.
pub fn load_suite(path: &str) -> Result<Vec<BenchQuestion>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Cannot read suite file: {path}"))?;

    let mut questions = Vec::new();
    for (line_no, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let q: BenchQuestion = serde_json::from_str(line)
            .with_context(|| format!("Invalid JSON on line {} of {path}", line_no + 1))?;
        questions.push(q);
    }

    if questions.is_empty() {
        anyhow::bail!("Suite file is empty: {path}");
    }

    Ok(questions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_question() {
        let json = r#"{"id":"q01","question":"What version?","ground_truth":["4.15.0"],"difficulty":"easy","category":"factual"}"#;
        let q: BenchQuestion = serde_json::from_str(json).unwrap();
        assert_eq!(q.id, "q01");
        assert_eq!(q.difficulty, Difficulty::Easy);
        assert_eq!(q.ground_truth, vec!["4.15.0"]);
        assert!(q.expected_files.is_empty());
    }

    #[test]
    fn test_deserialize_with_expected() {
        let json = r#"{"id":"q02","question":"Where is X?","ground_truth":["foo"],"expected_files":["src/lib.rs"],"expected_symbols":["Foo"],"difficulty":"medium"}"#;
        let q: BenchQuestion = serde_json::from_str(json).unwrap();
        assert_eq!(q.expected_files, vec!["src/lib.rs"]);
        assert_eq!(q.expected_symbols, vec!["Foo"]);
        assert_eq!(q.category, "general"); // default
    }

    #[test]
    fn test_load_suite_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");
        std::fs::write(
            &path,
            r#"# comment line
{"id":"q01","question":"Q1?","ground_truth":["a"],"difficulty":"easy"}
{"id":"q02","question":"Q2?","ground_truth":["b","c"],"difficulty":"hard","category":"structural"}
"#,
        )
        .unwrap();

        let suite = load_suite(path.to_str().unwrap()).unwrap();
        assert_eq!(suite.len(), 2);
        assert_eq!(suite[0].id, "q01");
        assert_eq!(suite[1].difficulty, Difficulty::Hard);
        assert_eq!(suite[1].category, "structural");
    }

    #[test]
    fn test_load_suite_empty_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.jsonl");
        std::fs::write(&path, "# only comments\n\n").unwrap();
        assert!(load_suite(path.to_str().unwrap()).is_err());
    }
}
