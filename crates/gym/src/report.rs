use serde::{Deserialize, Serialize};

/// The result of evaluating a [`Scenario`] in the Gym.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    /// Whether the overall evaluation succeeded (compiled + tests passed).
    pub success: bool,

    /// Compilation result.
    pub compilation: CompilationResult,

    /// Test execution result (None if no tests were provided).
    pub tests: Option<TestResult>,

    /// Performance metrics.
    pub metrics: Metrics,

    /// Fuzz testing result (None if fuzzing was not enabled or no functions were fuzzable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fuzz: Option<FuzzResult>,

    /// Adversarial mutation testing result (None if not enabled).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adversarial: Option<crate::adversarial::AdversarialResult>,

    /// Raw stderr from cargo (useful for debugging).
    #[serde(default)]
    pub raw_stderr: String,
}

/// Compilation outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationResult {
    /// Did it compile successfully?
    pub compiled: bool,
    /// Number of compiler warnings.
    pub warnings: u32,
    /// Number of compiler errors.
    pub errors: u32,
    /// Parsed error/warning messages from cargo JSON output.
    pub messages: Vec<CompilerMessage>,
}

/// A single compiler diagnostic message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilerMessage {
    pub level: String,
    pub message: String,
}

/// Test execution outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub passed: u32,
    pub failed: u32,
    pub ignored: u32,
    pub total: u32,
    /// Test output (stdout + stderr combined).
    pub output: String,
}

/// Performance metrics collected during evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metrics {
    /// Time to compile (milliseconds).
    pub compile_time_ms: u64,
    /// Binary/library size (bytes). 0 if compilation failed.
    pub binary_size_bytes: u64,
    /// Test execution time (milliseconds). 0 if no tests.
    pub test_time_ms: u64,
}

/// Result of proptest fuzz testing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuzzResult {
    /// Number of public functions that were fuzz-tested.
    pub fuzzed_functions: usize,
    /// Failures discovered by proptest (empty if all passed).
    pub failures: Vec<FuzzFailure>,
}

/// A single proptest failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuzzFailure {
    /// Name of the fuzz test function (e.g., "fuzz_add").
    pub function: String,
    /// The minimal failing input found by proptest shrinking.
    pub failing_input: String,
    /// The panic/error message.
    pub error: String,
}

impl Report {
    /// Compute a composite score (0.0 – 1.0) for ranking code variants.
    ///
    /// Scoring:
    /// - 0.0 if compilation failed
    /// - Base 0.5 for successful compilation
    /// - +0.3 for all tests passing (proportional to pass rate)
    /// - +0.1 for zero warnings
    /// - +0.1 for fast compilation (<5s)
    pub fn score(&self) -> f64 {
        if !self.compilation.compiled {
            return 0.0;
        }

        let mut score = 0.5;

        // Test pass rate (0.0 – 0.3)
        if let Some(ref tests) = self.tests {
            if tests.total > 0 {
                score += 0.3 * (tests.passed as f64 / tests.total as f64);
            } else {
                score += 0.3; // No tests = full credit for this component
            }
        } else {
            score += 0.3;
        }

        // Zero warnings bonus
        if self.compilation.warnings == 0 {
            score += 0.1;
        }

        // Fast compile bonus (<5 seconds)
        if self.metrics.compile_time_ms < 5000 {
            score += 0.1;
        }

        // Adversarial mutation score bonus (if enabled).
        // Replaces up to 0.1 of the speed bonus with mutation effectiveness.
        if let Some(ref adv) = self.adversarial {
            if adv.total_mutations > 0 {
                // Blend: keep base score, add up to 0.1 for high mutation score.
                score = (score - 0.1) + 0.1 * adv.mutation_score;
            }
        }

        score
    }
}
