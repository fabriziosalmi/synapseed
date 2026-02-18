//! Integration tests for the Gym sandbox.

use synapseed_gym::{Scenario, Trainer};

#[test]
fn test_add_function_compiles_and_passes() {
    let trainer = Trainer::new();

    let scenario = Scenario::new("pub fn add(a: i32, b: i32) -> i32 { a + b }")
        .with_tests(
            r#"
use eval_project::add;

#[test]
fn test_add_positive() {
    assert_eq!(add(2, 2), 4);
}

#[test]
fn test_add_negative() {
    assert_eq!(add(-1, 1), 0);
}

#[test]
fn test_add_zero() {
    assert_eq!(add(0, 0), 0);
}
"#,
        )
        .with_timeout(120);

    let report = trainer
        .evaluate(&scenario)
        .expect("Evaluation should succeed");

    assert!(report.compilation.compiled, "Code should compile");
    assert_eq!(report.compilation.errors, 0, "No compilation errors");
    assert!(report.success, "All tests should pass");

    let tests = report.tests.as_ref().expect("Tests should be present");
    assert_eq!(tests.passed, 3, "3 tests should pass");
    assert_eq!(tests.failed, 0, "0 tests should fail");
    assert_eq!(tests.total, 3, "3 total tests");

    assert!(
        report.metrics.compile_time_ms > 0,
        "Compile time should be measured"
    );

    let score = report.score();
    assert!(
        score > 0.9,
        "Perfect code should score > 0.9, got {score:.2}"
    );
}

#[test]
fn test_compilation_failure_returns_errors() {
    let trainer = Trainer::new();

    let scenario = Scenario::new("pub fn broken( { this is not valid rust }");

    let report = trainer
        .evaluate(&scenario)
        .expect("Evaluation should not panic");

    assert!(!report.compilation.compiled, "Should fail to compile");
    assert!(report.compilation.errors > 0, "Should have errors");
    assert!(!report.success, "Overall success should be false");
    assert_eq!(report.score(), 0.0, "Failed compilation = score 0");
}

#[test]
fn test_failing_tests_reported() {
    let trainer = Trainer::new();

    let scenario = Scenario::new(
        "pub fn multiply(a: i32, b: i32) -> i32 { a + b }", // Bug: add instead of multiply
    )
    .with_tests(
        r#"
use eval_project::multiply;

#[test]
fn test_multiply_correct() {
    assert_eq!(multiply(3, 4), 12);  // Will fail: 3+4=7, not 12
}
"#,
    );

    let report = trainer
        .evaluate(&scenario)
        .expect("Evaluation should not panic");

    assert!(report.compilation.compiled, "Should compile");
    assert!(!report.success, "Should fail (test failure)");

    let tests = report.tests.as_ref().expect("Tests should be present");
    assert_eq!(tests.failed, 1, "1 test should fail");
    assert!(report.score() < 0.9, "Failing tests should lower score");
}

#[test]
fn test_compare_variants() {
    let trainer = Trainer::new();

    let good = Scenario::new("pub fn double(x: i32) -> i32 { x * 2 }")
        .with_tests("use eval_project::double;\n#[test]\nfn t() { assert_eq!(double(5), 10); }");

    let bad =
        Scenario::new("pub fn double(x: i32) -> i32 { x + x + 1 }") // Off-by-one
            .with_tests(
                "use eval_project::double;\n#[test]\nfn t() { assert_eq!(double(5), 10); }",
            );

    let results = trainer.compare(&[good, bad]);

    assert_eq!(results.len(), 2);
    // The good variant (index 0) should rank first
    assert_eq!(results[0].0, 0, "Good variant should rank first");
    assert!(results[0].1.success);
    assert!(!results[1].1.success);
}
