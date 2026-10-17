//! Intent classification — weighted keyword heuristics (v5.0.0).
//!
//! Maps a natural-language query to an `Intent` enum variant by counting
//! **weighted** keyword matches across bug/security/explain/refactor categories.
//! High-signal keywords (e.g., "crash", "panic") carry more weight than
//! low-signal ones (e.g., "fix", "error"). Supports English and Italian.

use super::Intent;

/// Weighted keyword: (pattern, weight). Higher weight = stronger signal.
const BUG_KEYWORDS: &[(&str, u32)] = &[
    ("crash", 3),
    ("panic", 3),
    ("segfault", 3),
    ("abort", 3),
    ("bug", 2),
    ("broken", 2),
    ("fail", 2),
    ("wrong", 2),
    ("cannot", 2),
    ("compile", 2),
    ("rott", 2),
    ("traceback", 2),
    ("exception", 2),
    ("fix", 1),
    ("error", 1),
    ("issue", 1),
    // Italian
    ("errore", 2),
    ("rotto", 2),
    ("problema", 1),
    ("fallisce", 2),
    ("non funziona", 3),
];

const SECURITY_KEYWORDS: &[(&str, u32)] = &[
    ("cve", 3),
    ("injection", 3),
    ("xss", 3),
    ("rce", 3),
    ("ssrf", 3),
    ("vulnerability", 3),
    ("exploit", 3),
    ("security", 2),
    ("audit", 2),
    ("secret", 2),
    ("password", 2),
    ("leak", 2),
    ("token", 1),
    ("key", 1),
    ("vuln", 2),
    // Italian
    ("sicurezza", 2),
    ("segreto", 2),
    ("vulnerabilità", 3),
];

const EXPLAIN_KEYWORDS: &[(&str, u32)] = &[
    ("explain", 2),
    ("what is", 2),
    ("how does", 2),
    ("why", 1),
    ("understand", 2),
    ("describe", 2),
    ("what does", 2),
    ("walk through", 2),
    ("overview", 2),
    ("architecture", 2),
    // Italian
    ("cos'è", 2),
    ("perché", 1),
    ("come funziona", 2),
    ("spiega", 2),
    ("descrivi", 2),
    ("mostrami", 2),
];

const REFACTOR_KEYWORDS: &[(&str, u32)] = &[
    ("refactor", 2),
    ("optimize", 2),
    ("restructure", 2),
    ("simplify", 2),
    ("clean", 1),
    ("improve", 1),
    ("extract", 1),
    ("rename", 1),
    ("move", 1),
    ("performance", 2),
    ("speed up", 2),
    ("faster", 2),
    ("slow", 2),
    // Italian
    ("migliora", 2),
    ("pulisci", 1),
    ("ottimizza", 2),
    ("velocizza", 2),
    ("velocità", 2),
    ("intelligenza", 2),
    ("più veloce", 2),
    ("lento", 2),
];

/// Score a category by summing weights of matched keywords.
fn score_category(lower: &str, keywords: &[(&str, u32)]) -> u32 {
    keywords
        .iter()
        .filter(|(k, _)| lower.contains(*k))
        .map(|(_, w)| *w)
        .sum()
}

pub(super) fn classify_intent(query: &str) -> Intent {
    let lower = query.to_lowercase();

    let bug_score = score_category(&lower, BUG_KEYWORDS);
    let sec_score = score_category(&lower, SECURITY_KEYWORDS);
    let exp_score = score_category(&lower, EXPLAIN_KEYWORDS);
    let ref_score = score_category(&lower, REFACTOR_KEYWORDS);

    let max = bug_score.max(sec_score).max(exp_score).max(ref_score);

    if max == 0 {
        return Intent::General;
    }

    // Highest score wins; on tie, order of priority: BugFix > Security > Explain > Refactor
    if bug_score == max {
        Intent::BugFix
    } else if sec_score == max {
        Intent::Security
    } else if exp_score == max {
        Intent::Explain
    } else {
        Intent::Refactor
    }
}

/// Return all non-zero intent scores sorted by score descending (v4.12.0).
/// The first entry is the winner; subsequent entries carry secondary signals
/// (e.g., "fix the security bug" → [("bug_fix", 5), ("security", 2)]).
pub(super) fn classify_intent_scores(query: &str) -> Vec<(String, usize)> {
    let lower = query.to_lowercase();

    let bug_score = score_category(&lower, BUG_KEYWORDS) as usize;
    let sec_score = score_category(&lower, SECURITY_KEYWORDS) as usize;
    let exp_score = score_category(&lower, EXPLAIN_KEYWORDS) as usize;
    let ref_score = score_category(&lower, REFACTOR_KEYWORDS) as usize;

    let mut scores: Vec<(String, usize)> = vec![
        ("bug_fix".to_string(), bug_score),
        ("security".to_string(), sec_score),
        ("explain".to_string(), exp_score),
        ("refactor".to_string(), ref_score),
    ];

    // Keep only non-zero, sort descending
    scores.retain(|(_, s)| *s > 0);
    scores.sort_by_key(|s| std::cmp::Reverse(s.1));
    scores
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_bug_fix() {
        assert!(matches!(
            classify_intent("fix the broken login"),
            Intent::BugFix
        ));
        assert!(matches!(
            classify_intent("why is this error happening"),
            Intent::BugFix
        ));
        assert!(matches!(
            classify_intent("the code fails to compile"),
            Intent::BugFix
        ));
    }

    #[test]
    fn test_classify_security() {
        assert!(matches!(
            classify_intent("run a security audit"),
            Intent::Security
        ));
        assert!(matches!(
            classify_intent("check for leaked secrets"),
            Intent::Security
        ));
        assert!(matches!(
            classify_intent("is there a password in the code"),
            Intent::Security
        ));
    }

    #[test]
    fn test_classify_explain() {
        assert!(matches!(
            classify_intent("explain the authentication flow"),
            Intent::Explain
        ));
        assert!(matches!(
            classify_intent("what is SynapseContext"),
            Intent::Explain
        ));
        assert!(matches!(
            classify_intent("how does the router work"),
            Intent::Explain
        ));
    }

    #[test]
    fn test_classify_refactor() {
        assert!(matches!(
            classify_intent("refactor the parser module"),
            Intent::Refactor
        ));
        assert!(matches!(
            classify_intent("optimize the search index"),
            Intent::Refactor
        ));
    }

    #[test]
    fn test_classify_general() {
        assert!(matches!(
            classify_intent("list all modules"),
            Intent::General
        ));
        assert!(matches!(
            classify_intent("show me the project structure"),
            Intent::General
        ));
    }

    #[test]
    fn test_classify_priority_on_tie() {
        // "fix" (BugFix) + "security" (Security) → BugFix wins on priority
        assert!(matches!(
            classify_intent("fix the security issue"),
            Intent::BugFix
        ));
    }

    #[test]
    fn test_classify_italian() {
        assert!(matches!(
            classify_intent("perché la login è rotta"),
            Intent::BugFix
        ));
        assert!(matches!(
            classify_intent("spiega come funziona il router"),
            Intent::Explain
        ));
    }
}
