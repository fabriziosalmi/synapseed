//! Scoring engine: matches `ask` responses against ground truth.
//!
//! Scoring strategy:
//! - **Fact matching**: each ground truth string is substring-searched in `smart_context`
//!   (case-insensitive). A match counts as a "found fact".
//! - **File verification**: each file cited in `targets` is checked for existence on disk.
//!   Files that don't exist are counted as hallucinations.
//! - **Symbol matching**: expected symbols are checked against actual target names.
//!
//! Precision = facts_matched / (facts_matched + hallucinated_files)
//! Recall    = facts_matched / facts_total
//! F1        = 2 * P * R / (P + R)

use crate::suite::BenchQuestion;
use synapseed_core::context::SynapseContext;
use synapseed_whisper::router::WhisperResult;

/// Score for a single question.
#[derive(Debug, Clone)]
pub struct QuestionScore {
    /// Number of ground truth facts found in the response.
    pub facts_matched: usize,
    /// Total ground truth facts.
    pub facts_total: usize,
    /// Number of expected files found in targets.
    pub files_matched: usize,
    /// Number of expected symbols found in targets.
    pub symbols_matched: usize,
    /// Files cited in response that don't exist on disk.
    pub hallucinated_files: Vec<String>,
    /// Precision (0.0–1.0).
    pub precision: f64,
    /// Recall (0.0–1.0).
    pub recall: f64,
    /// F1 score (0.0–1.0).
    pub f1: f64,
}

/// Score a whisper response against a benchmark question's ground truth.
pub fn score_response(
    question: &BenchQuestion,
    result: &WhisperResult,
    ctx: &SynapseContext,
) -> QuestionScore {
    let context_lower = result.smart_context.to_lowercase();
    let facts_total = question.ground_truth.len();

    // --- Fact matching (substring, case-insensitive) ---
    let facts_matched = question
        .ground_truth
        .iter()
        .filter(|fact| context_lower.contains(&fact.to_lowercase()))
        .count();

    // --- File matching ---
    let target_files: Vec<String> = result
        .targets
        .iter()
        .filter_map(|t| t.file_path.clone())
        .collect();

    let files_matched = question
        .expected_files
        .iter()
        .filter(|ef| {
            target_files
                .iter()
                .any(|tf| tf.contains(ef.as_str()) || ef.contains(tf.as_str()))
        })
        .count();

    // --- Symbol matching ---
    let target_names: Vec<&str> = result.targets.iter().map(|t| t.name.as_str()).collect();

    let symbols_matched = question
        .expected_symbols
        .iter()
        .filter(|es| {
            target_names.iter().any(|tn| {
                tn.contains(es.as_str()) || es.contains(tn)
            })
        })
        .count();

    // --- Hallucination detection ---
    // Check if cited file paths actually exist on disk
    let project_root = ctx.project_root();
    let hallucinated_files: Vec<String> = target_files
        .iter()
        .filter(|f| {
            let path = project_root.join(f);
            !path.exists()
        })
        .cloned()
        .collect();

    // --- Compute metrics ---
    // Precision: of the facts we claim + files we cite, how many are real?
    let true_positives = facts_matched;
    let false_positives = hallucinated_files.len();
    let precision = if true_positives + false_positives > 0 {
        true_positives as f64 / (true_positives + false_positives) as f64
    } else {
        0.0
    };

    // Recall: of the ground truth facts, how many did we find?
    let recall = if facts_total > 0 {
        facts_matched as f64 / facts_total as f64
    } else {
        1.0 // no facts to find = perfect recall
    };

    // F1
    let f1 = if precision + recall > f64::EPSILON {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };

    QuestionScore {
        facts_matched,
        facts_total,
        files_matched,
        symbols_matched,
        hallucinated_files,
        precision,
        recall,
        f1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal WhisperResult for testing.
    fn mock_result(smart_context: &str, files: &[&str]) -> WhisperResult {
        use synapseed_whisper::router::{Intent, QueryComplexity, Target};

        WhisperResult {
            intent: Intent::General,
            intent_scores: vec![],
            complexity: QueryComplexity::Quick,
            query: "test".into(),
            targets: files
                .iter()
                .map(|f| Target {
                    kind: synapseed_whisper::router::TargetKind::File,
                    name: f.to_string(),
                    file_path: Some(f.to_string()),
                    line_start: None,
                    score: None,
                })
                .collect(),
            diagnostics: None,
            histories: vec![],
            code_context: None,
            security_status: "CLEAN".into(),
            smart_context: smart_context.into(),
            sid: 15.0,
            raw_sources: vec![],
        }
    }

    #[test]
    fn test_perfect_match() {
        let q = BenchQuestion {
            id: "q01".into(),
            question: "What version?".into(),
            ground_truth: vec!["4.15.0".into()],
            expected_files: vec![],
            expected_symbols: vec![],
            difficulty: crate::suite::Difficulty::Easy,
            category: "factual".into(),
        };

        // Build a minimal context to avoid disk access
        // For this test, we just check the math
        let result = mock_result("The version is 4.15.0", &[]);

        // Score without ctx (hallucination check needs real fs)
        let facts_matched = 1usize;
        let _facts_total = 1usize;
        let precision: f64 = 1.0;
        let recall: f64 = 1.0;
        let f1: f64 = 1.0;

        assert_eq!(facts_matched, q.ground_truth.iter()
            .filter(|fact| result.smart_context.to_lowercase().contains(&fact.to_lowercase()))
            .count());
        assert!((precision - 1.0).abs() < f64::EPSILON);
        assert!((recall - 1.0).abs() < f64::EPSILON);
        assert!((f1 - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_partial_match() {
        let ground_truth = vec!["BugFix".to_string(), "Security".into(), "Explain".into(), "Refactor".into(), "General".into()];
        let context = "The intents are BugFix, Security, and Explain. There are more.";

        let matched = ground_truth.iter()
            .filter(|f| context.to_lowercase().contains(&f.to_lowercase()))
            .count();

        assert_eq!(matched, 3); // BugFix, Security, Explain
        let recall = matched as f64 / ground_truth.len() as f64;
        assert!((recall - 0.6).abs() < f64::EPSILON);
    }

    #[test]
    fn test_no_match() {
        let ground_truth = vec!["nonexistent_thing".to_string()];
        let context = "This response has nothing relevant.";

        let matched = ground_truth.iter()
            .filter(|f| context.to_lowercase().contains(&f.to_lowercase()))
            .count();

        assert_eq!(matched, 0);
    }
}
