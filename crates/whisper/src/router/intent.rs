//! Intent classification — deterministic keyword heuristics.
//!
//! Maps a natural-language query to an `Intent` enum variant by counting
//! keyword matches across bug/security/explain/refactor categories.
//! Supports both English and Italian keywords.

use super::Intent;

const BUG_KEYWORDS: &[&str] = &[
    "fix", "error", "broken", "bug", "crash", "fail", "wrong", "issue", "rott", "compile", "panic",
    "cannot", "errore", "rotto", "problema",
];

const SECURITY_KEYWORDS: &[&str] = &[
    "security",
    "audit",
    "secret",
    "password",
    "vuln",
    "leak",
    "token",
    "key",
    "cve",
    "xss",
    "injection",
    "sicurezza",
    "segreto",
];

const EXPLAIN_KEYWORDS: &[&str] = &[
    "explain",
    "what is",
    "how does",
    "why",
    "understand",
    "describe",
    "what does",
    "cos'è",
    "perché",
    "come funziona",
    "spiega",
];

const REFACTOR_KEYWORDS: &[&str] = &[
    "refactor",
    "clean",
    "improve",
    "optimize",
    "restructure",
    "simplify",
    "extract",
    "rename",
    "move",
    "migliora",
    "pulisci",
];

pub(super) fn classify_intent(query: &str) -> Intent {
    let lower = query.to_lowercase();

    // Score each intent by keyword matches (first match wins for ties)
    let bug_score = BUG_KEYWORDS.iter().filter(|k| lower.contains(**k)).count();
    let sec_score = SECURITY_KEYWORDS
        .iter()
        .filter(|k| lower.contains(**k))
        .count();
    let exp_score = EXPLAIN_KEYWORDS
        .iter()
        .filter(|k| lower.contains(**k))
        .count();
    let ref_score = REFACTOR_KEYWORDS
        .iter()
        .filter(|k| lower.contains(**k))
        .count();

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
