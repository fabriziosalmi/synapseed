//! Report data structures and JSON output.

use crate::suite::{Difficulty, QuestionCategory};
use serde::Serialize;

/// Full benchmark report.
#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkReport {
    pub metadata: ReportMetadata,
    pub questions: Vec<QuestionResult>,
    pub aggregate: AggregateMetrics,
}

/// Suite-level metadata.
#[derive(Debug, Clone, Serialize)]
pub struct ReportMetadata {
    pub suite_path: String,
    pub question_count: usize,
    pub project_root: String,
    pub version: String,
    pub timestamp: String,
}

/// Per-question result.
#[derive(Debug, Clone, Serialize)]
pub struct QuestionResult {
    pub id: String,
    pub question: String,
    pub difficulty: Difficulty,
    pub category: QuestionCategory,
    pub sid: f64,
    pub targets_found: usize,
    pub facts_matched: usize,
    pub facts_total: usize,
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
    pub hallucinated_files: Vec<String>,
    pub response_tokens: usize,
    /// Semantic Compression Ratio = Correct_Facts / Input_Tokens * 1000
    pub scr: f64,
}

/// Aggregate metrics across all questions.
#[derive(Debug, Clone, Serialize, Default)]
pub struct AggregateMetrics {
    pub mean_f1: f64,
    pub mean_precision: f64,
    pub mean_recall: f64,
    pub mean_sid: f64,
    /// Mean Semantic Compression Ratio.
    pub mean_scr: f64,
    /// Fraction of questions with at least one hallucinated file.
    pub hallucination_rate: f64,
    /// Pearson correlation between SID and F1 across questions.
    pub sid_f1_correlation: f64,
    pub questions_total: usize,
    pub perfect_scores: usize,
    pub zero_scores: usize,
    /// Mean F1 for easy questions.
    pub easy_mean_f1: f64,
    /// Mean F1 for medium questions.
    pub medium_mean_f1: f64,
    /// Mean F1 for hard questions.
    pub hard_mean_f1: f64,
}

impl BenchmarkReport {
    /// Serialize the report to pretty JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
    }

    /// Format a human-readable summary.
    pub fn summary(&self) -> String {
        let a = &self.aggregate;
        let mut out = String::with_capacity(1024);
        out.push_str(&format!("# Benchmark Report — {}\n\n", self.metadata.timestamp));
        out.push_str(&format!("**Suite**: {}\n", self.metadata.suite_path));
        out.push_str(&format!("**Project**: {} (v{})\n", self.metadata.project_root, self.metadata.version));
        out.push_str(&format!("**Questions**: {}\n\n", self.metadata.question_count));

        out.push_str("## Aggregate Metrics\n\n");
        out.push_str(&format!("| Metric | Value |\n|--------|-------|\n"));
        out.push_str(&format!("| F1 (mean) | {:.3} |\n", a.mean_f1));
        out.push_str(&format!("| Precision (mean) | {:.3} |\n", a.mean_precision));
        out.push_str(&format!("| Recall (mean) | {:.3} |\n", a.mean_recall));
        out.push_str(&format!("| SID (mean) | {:.1} |\n", a.mean_sid));
        out.push_str(&format!("| SCR (mean) | {:.2} |\n", a.mean_scr));
        out.push_str(&format!("| Hallucination Rate | {:.1}% |\n", a.hallucination_rate * 100.0));
        out.push_str(&format!("| SID↔F1 Correlation | {:.3} |\n", a.sid_f1_correlation));
        out.push_str(&format!("| Perfect Scores | {} |\n", a.perfect_scores));
        out.push_str(&format!("| Zero Scores | {} |\n\n", a.zero_scores));

        out.push_str("## Difficulty Breakdown\n\n");
        out.push_str(&format!("| Difficulty | Mean F1 |\n|-----------|--------|\n"));
        out.push_str(&format!("| Easy | {:.3} |\n", a.easy_mean_f1));
        out.push_str(&format!("| Medium | {:.3} |\n", a.medium_mean_f1));
        out.push_str(&format!("| Hard | {:.3} |\n\n", a.hard_mean_f1));

        out.push_str("## Per-Question Results\n\n");
        out.push_str("| ID | Difficulty | F1 | SID | SCR | Facts | Hallucinations |\n");
        out.push_str("|-----|-----------|------|------|------|-------|----------------|\n");
        for q in &self.questions {
            let flag = if q.f1 >= 0.8 { "✅" } else if q.f1 >= 0.5 { "⚠️" } else { "❌" };
            out.push_str(&format!(
                "| {flag} {} | {:?} | {:.2} | {:.1} | {:.2} | {}/{} | {} |\n",
                q.id, q.difficulty, q.f1, q.sid, q.scr,
                q.facts_matched, q.facts_total,
                q.hallucinated_files.len()
            ));
        }

        out
    }
}
