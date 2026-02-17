#![forbid(unsafe_code)]
//! # SYNAPSEED Gym
//!
//! An isolated RL sandbox for compiling, testing, and benchmarking Rust code.
//!
//! The Gym takes a [`Scenario`] (source code + optional tests + dependencies),
//! spins up an ephemeral Cargo project in a temp directory, and returns a
//! [`Report`] with compilation status, test results, and performance metrics.
//!
//! ## Usage
//!
//! ```no_run
//! use synapseed_gym::{Trainer, Scenario};
//!
//! let trainer = Trainer::new();
//! let scenario = Scenario::new("pub fn add(a: i32, b: i32) -> i32 { a + b }")
//!     .with_tests("use eval_project::add;\n#[test]\nfn test_add() { assert_eq!(add(2, 2), 4); }");
//!
//! let report = trainer.evaluate(&scenario).unwrap();
//! assert!(report.success);
//! println!("Score: {:.2}", report.score());
//! ```

pub(crate) mod adversarial;
pub(crate) mod fuzzer;
pub mod plugin;
pub(crate) mod report;
pub(crate) mod sandbox;
pub(crate) mod scenario;

pub use report::Report;
pub use scenario::Scenario;

use sandbox::Sandbox;

/// Error types for Gym operations.
#[derive(Debug, thiserror::Error)]
pub enum GymError {
    #[error("Sandbox error: {0}")]
    Sandbox(String),

    #[error("Build error: {0}")]
    Build(String),

    #[error("Evaluation timed out after {0}s")]
    Timeout(u64),
}

/// The Trainer — entry point for evaluating code in the Gym.
///
/// Each call to [`evaluate`] creates a fresh, isolated sandbox.
pub struct Trainer;

impl Trainer {
    pub fn new() -> Self {
        Self
    }

    /// Evaluate a scenario: compile, test, and measure.
    ///
    /// Returns a [`Report`] with results and a composite score.
    pub fn evaluate(&self, scenario: &Scenario) -> Result<Report, GymError> {
        let sandbox = Sandbox::create()?;
        sandbox.evaluate(scenario)
    }

    /// Compare multiple code variants and rank them by score.
    ///
    /// Returns reports sorted by descending score (best first).
    pub fn compare(&self, scenarios: &[Scenario]) -> Vec<(usize, Report)> {
        let mut results: Vec<(usize, Report)> = scenarios
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                match self.evaluate(s) {
                    Ok(report) => Some((i, report)),
                    Err(e) => {
                        tracing::warn!(index = i, error = %e, "Scenario evaluation failed");
                        None
                    }
                }
            })
            .collect();

        results.sort_by(|a, b| {
            b.1.score()
                .partial_cmp(&a.1.score())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }
}

impl Default for Trainer {
    fn default() -> Self {
        Self::new()
    }
}
