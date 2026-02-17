//! Adversarial mutation testing — the Saboteur.
//!
//! Applies controlled mutations to source code and re-evaluates,
//! measuring how well the test suite detects intentional defects.
//!
//! Mutation strategies:
//! - ArithmeticSwap:  `+` → `-`, `*` → `/`
//! - BooleanNegate:   `==` → `!=`, `true` → `false`
//! - BoundaryShift:   numeric literals ±1
//! - ReturnRemove:    replace return expression with default
//! - StatementDelete: remove non-trivial statements

use serde::{Deserialize, Serialize};

/// A mutation strategy applied by the Saboteur.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationStrategy {
    /// Replace arithmetic operators: `+` → `-`, `*` → `/`.
    ArithmeticSwap,
    /// Negate boolean conditions: `==` → `!=`, `true` → `false`.
    BooleanNegate,
    /// Shift numeric boundary values by ±1 (off-by-one).
    BoundaryShift,
    /// Replace return expression with type default.
    ReturnRemove,
    /// Delete a non-trivial statement entirely.
    StatementDelete,
}

/// A single mutation applied to source code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mutation {
    pub strategy: MutationStrategy,
    pub description: String,
    pub line: usize,
    pub original: String,
    pub mutated: String,
}

/// Result of adversarial mutation testing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdversarialResult {
    /// Total mutations applied.
    pub total_mutations: usize,
    /// Mutations detected by tests (test suite caught the defect).
    pub detected: usize,
    /// Mutations that survived (test suite did NOT catch the defect).
    pub survived: usize,
    /// Mutation score: detected / total. 1.0 = all mutations caught.
    pub mutation_score: f64,
    /// Details of each mutation attempt.
    pub mutations: Vec<MutationOutcome>,
}

impl AdversarialResult {
    /// Generate Rust test stubs for mutations that survived (i.e. tests didn't
    /// catch the defect). These stubs are scaffolding for the developer to
    /// fill in, turning weaknesses into permanent regression tests.
    pub fn generate_repro_tests(&self) -> Option<String> {
        let survived: Vec<&MutationOutcome> =
            self.mutations.iter().filter(|o| !o.detected).collect();
        if survived.is_empty() {
            return None;
        }

        let mut out = String::from(
            "//! Auto-generated regression test stubs for survived mutations.\n\
             //! The Saboteur found mutations your tests couldn't catch.\n\
             //! Fill in the TODO bodies to harden your test suite.\n\n\
             use eval_project::*;\n\n",
        );

        for (i, outcome) in survived.iter().enumerate() {
            let m = &outcome.mutation;
            let strategy = format!("{:?}", m.strategy);
            out.push_str(&format!(
                "/// SURVIVED: {strategy} at line {line}\n\
                 /// Original: `{original}`\n\
                 /// Mutated:  `{mutated}`\n\
                 #[test]\n\
                 fn repro_survived_mutation_{idx}() {{\n\
                 {indent}// TODO: add an assertion that would catch the {strategy} mutation\n\
                 {indent}// on line {line}. The mutant changed:\n\
                 {indent}//   {original}\n\
                 {indent}// to:\n\
                 {indent}//   {mutated}\n\
                 {indent}todo!(\"harden: catch {strategy} at line {line}\")\n\
                 }}\n\n",
                strategy = strategy,
                line = m.line,
                original = m.original.trim(),
                mutated = m.mutated.trim(),
                idx = i + 1,
                indent = "    ",
            ));
        }

        Some(out)
    }
}

/// Outcome of a single mutation attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationOutcome {
    pub mutation: Mutation,
    /// Whether the test suite detected this mutation (test failed → detected).
    pub detected: bool,
}

/// The Saboteur — generates source code mutations for adversarial testing.
pub struct Saboteur;

impl Saboteur {
    /// Generate all applicable mutations for the given source code.
    ///
    /// Returns at most 20 mutations to keep evaluation time bounded.
    pub fn generate_mutations(source: &str) -> Vec<Mutation> {
        let mut mutations = Vec::new();
        let lines: Vec<&str> = source.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Skip comments and empty lines.
            if trimmed.is_empty()
                || trimmed.starts_with("//")
                || trimmed.starts_with('#')
                || trimmed.starts_with("use ")
                || trimmed.starts_with("mod ")
            {
                continue;
            }

            // ArithmeticSwap
            if let Some(mutated) = swap_arithmetic(line) {
                mutations.push(Mutation {
                    strategy: MutationStrategy::ArithmeticSwap,
                    description: format!("Swap arithmetic operator on line {}", i + 1),
                    line: i + 1,
                    original: line.to_string(),
                    mutated,
                });
            }

            // BooleanNegate
            if let Some(mutated) = negate_boolean(line) {
                mutations.push(Mutation {
                    strategy: MutationStrategy::BooleanNegate,
                    description: format!("Negate boolean condition on line {}", i + 1),
                    line: i + 1,
                    original: line.to_string(),
                    mutated,
                });
            }

            // BoundaryShift
            if let Some(mutated) = shift_boundary(trimmed) {
                mutations.push(Mutation {
                    strategy: MutationStrategy::BoundaryShift,
                    description: format!("Shift boundary value on line {}", i + 1),
                    line: i + 1,
                    original: line.to_string(),
                    mutated: format!(
                        "{}{}",
                        &line[..line.len() - trimmed.len()], // preserve leading whitespace
                        mutated
                    ),
                });
            }

            // ReturnRemove
            if let Some(mutated) = remove_return(trimmed) {
                mutations.push(Mutation {
                    strategy: MutationStrategy::ReturnRemove,
                    description: format!("Remove return expression on line {}", i + 1),
                    line: i + 1,
                    original: line.to_string(),
                    mutated: format!(
                        "{}{}",
                        &line[..line.len() - trimmed.len()],
                        mutated
                    ),
                });
            }

            // StatementDelete
            if is_deletable_statement(trimmed) {
                mutations.push(Mutation {
                    strategy: MutationStrategy::StatementDelete,
                    description: format!("Delete statement on line {}", i + 1),
                    line: i + 1,
                    original: line.to_string(),
                    mutated: String::new(),
                });
            }
        }

        mutations.truncate(20);
        mutations
    }

    /// Apply a single mutation to the source code, returning the mutated source.
    pub fn apply_mutation(source: &str, mutation: &Mutation) -> String {
        let lines: Vec<&str> = source.lines().collect();
        let mut result = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            if i + 1 == mutation.line {
                if mutation.mutated.is_empty() {
                    continue; // StatementDelete
                }
                result.push(mutation.mutated.as_str());
            } else {
                result.push(line);
            }
        }

        result.join("\n")
    }
}

// ── Mutation Helpers ───────────────────────────────────────────────

fn swap_arithmetic(line: &str) -> Option<String> {
    let l = line.trim();
    if l.starts_with("//") || l.starts_with("///") {
        return None;
    }
    if line.contains(" + ") {
        Some(line.replacen(" + ", " - ", 1))
    } else if line.contains(" - ") && !line.contains("->") && !line.contains("// ") {
        Some(line.replacen(" - ", " + ", 1))
    } else if line.contains(" * ") {
        Some(line.replacen(" * ", " / ", 1))
    } else {
        None
    }
}

fn negate_boolean(line: &str) -> Option<String> {
    let l = line.trim();
    if l.starts_with("//") || l.starts_with("///") {
        return None;
    }
    if line.contains(" == ") {
        Some(line.replacen(" == ", " != ", 1))
    } else if line.contains(" != ") {
        Some(line.replacen(" != ", " == ", 1))
    } else if line.contains(" >= ") {
        Some(line.replacen(" >= ", " < ", 1))
    } else if line.contains(" <= ") {
        Some(line.replacen(" <= ", " > ", 1))
    } else if line.contains(" > ") && !line.contains("->") {
        Some(line.replacen(" > ", " <= ", 1))
    } else if line.contains(" < ") && !line.contains("<-") {
        Some(line.replacen(" < ", " >= ", 1))
    } else {
        None
    }
}

fn shift_boundary(line: &str) -> Option<String> {
    if line.starts_with("//") {
        return None;
    }
    let mut result = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    let mut shifted = false;

    while let Some(c) = chars.next() {
        if !shifted && c.is_ascii_digit() {
            let mut num = String::new();
            num.push(c);
            while let Some(&next) = chars.peek() {
                if next.is_ascii_digit() {
                    num.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            if let Ok(n) = num.parse::<i64>() {
                result.push_str(&(n + 1).to_string());
                shifted = true;
            } else {
                result.push_str(&num);
            }
        } else {
            result.push(c);
        }
    }

    if shifted {
        Some(result)
    } else {
        None
    }
}

fn remove_return(line: &str) -> Option<String> {
    if line.starts_with("return ") {
        Some("return Default::default();".to_string())
    } else {
        None
    }
}

fn is_deletable_statement(line: &str) -> bool {
    let l = line.trim();
    l.ends_with(';')
        && !l.starts_with("//")
        && !l.starts_with("use ")
        && !l.starts_with("mod ")
        && !l.starts_with("pub ")
        && !l.starts_with("let mut ")
        && !l.starts_with("let ")
        && !l.starts_with('#')
        && l.len() > 10
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arithmetic_swap() {
        assert_eq!(
            swap_arithmetic("    let x = a + b;"),
            Some("    let x = a - b;".to_string())
        );
        assert_eq!(
            swap_arithmetic("    let x = a * b;"),
            Some("    let x = a / b;".to_string())
        );
    }

    #[test]
    fn test_boolean_negate() {
        assert_eq!(
            negate_boolean("    if x == 0 {"),
            Some("    if x != 0 {".to_string())
        );
        assert_eq!(
            negate_boolean("    if x > 0 {"),
            Some("    if x <= 0 {".to_string())
        );
    }

    #[test]
    fn test_boundary_shift() {
        assert_eq!(
            shift_boundary("if count > 10 {"),
            Some("if count > 11 {".to_string())
        );
    }

    #[test]
    fn test_generate_mutations() {
        let source = "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}";
        let mutations = Saboteur::generate_mutations(source);
        assert!(!mutations.is_empty(), "Expected at least one mutation");
    }

    #[test]
    fn test_apply_mutation() {
        let source = "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}";
        let mutation = Mutation {
            strategy: MutationStrategy::ArithmeticSwap,
            description: "swap + to -".to_string(),
            line: 2,
            original: "    a + b".to_string(),
            mutated: "    a - b".to_string(),
        };
        let result = Saboteur::apply_mutation(source, &mutation);
        assert!(result.contains("a - b"));
        assert!(!result.contains("a + b"));
    }
}
