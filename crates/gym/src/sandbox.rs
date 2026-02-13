use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use tempfile::TempDir;
use tracing::{debug, warn};

use crate::adversarial::{AdversarialResult, MutationOutcome, Saboteur};
use crate::fuzzer;
use crate::report::{CompilationResult, CompilerMessage, FuzzFailure, FuzzResult, Metrics, Report, TestResult};
use crate::scenario::Scenario;

/// An isolated sandbox environment for compiling and testing Rust code.
pub struct Sandbox {
    _dir: TempDir,
    project_path: PathBuf,
}

impl Sandbox {
    /// Create a new sandbox with `cargo init --lib` in a temp directory.
    pub fn create() -> Result<Self, crate::GymError> {
        let dir = TempDir::new().map_err(|e| crate::GymError::Sandbox(e.to_string()))?;
        let project_path = dir.path().join("eval_project");

        let output = Command::new("cargo")
            .args(["init", "--lib", "--name", "eval_project"])
            .arg(&project_path)
            .output()
            .map_err(|e| crate::GymError::Sandbox(format!("cargo init failed: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(crate::GymError::Sandbox(format!(
                "cargo init failed: {stderr}"
            )));
        }

        // Configure cargo to use the global registry cache (no re-downloads).
        // SECURITY: Force offline mode to prevent AI-generated code from
        // downloading payloads or exfiltrating data via network during eval.
        let cargo_dir = project_path.join(".cargo");
        std::fs::create_dir_all(&cargo_dir)
            .map_err(|e| crate::GymError::Sandbox(e.to_string()))?;

        std::fs::write(
            cargo_dir.join("config.toml"),
            "[net]\noffline = true\n\n[build]\nincremental = false\n",
        )
        .map_err(|e| crate::GymError::Sandbox(e.to_string()))?;

        debug!(path = %project_path.display(), "Sandbox created");

        Ok(Self {
            _dir: dir,
            project_path,
        })
    }

    /// Inject source code and test code into the sandbox project.
    pub fn inject(&self, scenario: &Scenario) -> Result<(), crate::GymError> {
        // Write src/lib.rs
        let src_path = self.project_path.join("src/lib.rs");
        std::fs::write(&src_path, &scenario.source_code)
            .map_err(|e| crate::GymError::Sandbox(format!("Failed to write src/lib.rs: {e}")))?;

        // Write tests/eval.rs if test code provided
        if !scenario.test_code.is_empty() {
            let tests_dir = self.project_path.join("tests");
            std::fs::create_dir_all(&tests_dir)
                .map_err(|e| crate::GymError::Sandbox(e.to_string()))?;
            std::fs::write(tests_dir.join("eval.rs"), &scenario.test_code)
                .map_err(|e| crate::GymError::Sandbox(format!("Failed to write test: {e}")))?;
        }

        // Generate and inject fuzz tests if enabled.
        let fuzz_generated = if scenario.fuzz {
            if let Some(fuzz_code) = fuzzer::generate_fuzz_tests(&scenario.source_code) {
                let tests_dir = self.project_path.join("tests");
                std::fs::create_dir_all(&tests_dir)
                    .map_err(|e| crate::GymError::Sandbox(e.to_string()))?;
                std::fs::write(tests_dir.join("fuzz.rs"), &fuzz_code)
                    .map_err(|e| crate::GymError::Sandbox(format!("Failed to write fuzz tests: {e}")))?;
                debug!(bytes = fuzz_code.len(), "Injected fuzz tests");
                true
            } else {
                debug!("No fuzzable functions found, skipping fuzz injection");
                false
            }
        } else {
            false
        };

        // Update Cargo.toml with extra dependencies + proptest if fuzzing.
        if !scenario.dependencies.is_empty() || fuzz_generated {
            self.add_dependencies(scenario, fuzz_generated)?;
        }

        debug!("Injected source ({} bytes) + tests ({} bytes)",
            scenario.source_code.len(), scenario.test_code.len());

        Ok(())
    }

    /// Build the project with `cargo check --message-format=json`.
    pub fn build(&self, timeout_secs: u64) -> Result<(CompilationResult, u64), crate::GymError> {
        let start = Instant::now();

        let output = Command::new("cargo")
            .args(["check", "--message-format=json"])
            .current_dir(&self.project_path)
            .env("CARGO_TERM_COLOR", "never")
            .output()
            .map_err(|e| crate::GymError::Build(format!("cargo check failed to start: {e}")))?;

        let compile_time_ms = start.elapsed().as_millis() as u64;

        if compile_time_ms > timeout_secs * 1000 {
            return Err(crate::GymError::Timeout(timeout_secs));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let _stderr = String::from_utf8_lossy(&output.stderr);

        // Parse cargo JSON messages from stdout
        let mut messages = Vec::new();
        let mut warnings = 0u32;
        let mut errors = 0u32;

        for line in stdout.lines() {
            if let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) {
                if msg.get("reason").and_then(|r| r.as_str()) == Some("compiler-message") {
                    if let Some(inner) = msg.get("message") {
                        let level = inner
                            .get("level")
                            .and_then(|l| l.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let text = inner
                            .get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("")
                            .to_string();

                        match level.as_str() {
                            "warning" => warnings += 1,
                            "error" => errors += 1,
                            _ => {}
                        }

                        messages.push(CompilerMessage {
                            level,
                            message: text,
                        });
                    }
                }
            }
        }

        let compiled = output.status.success();

        if !compiled {
            debug!(errors, warnings, "Compilation failed");
        } else {
            debug!(warnings, compile_ms = compile_time_ms, "Compilation succeeded");
        }

        Ok((
            CompilationResult {
                compiled,
                warnings,
                errors,
                messages,
            },
            compile_time_ms,
        ))
    }

    /// Run tests with `cargo test`.
    pub fn test(&self, timeout_secs: u64) -> Result<(TestResult, u64), crate::GymError> {
        let start = Instant::now();

        let output = Command::new("cargo")
            .args(["test", "--", "--nocapture"])
            .current_dir(&self.project_path)
            .env("CARGO_TERM_COLOR", "never")
            .output()
            .map_err(|e| crate::GymError::Build(format!("cargo test failed to start: {e}")))?;

        let test_time_ms = start.elapsed().as_millis() as u64;

        if test_time_ms > timeout_secs * 1000 {
            return Err(crate::GymError::Timeout(timeout_secs));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{stdout}\n{stderr}");

        // Parse test results from output (aggregated across all test binaries).
        let (passed, failed, ignored, total) = parse_test_summary(&combined);

        if total == 0 && output.status.success() {
            debug!(stdout_len = stdout.len(), stderr_len = stderr.len(),
                "No test summary found in output despite successful exit");
        }

        debug!(passed, failed, ignored, test_ms = test_time_ms, "Tests completed");

        Ok((
            TestResult {
                passed,
                failed,
                ignored,
                total,
                output: combined,
            },
            test_time_ms,
        ))
    }

    /// Measure the library artifact size after `cargo build --release`.
    pub fn measure_binary_size(&self) -> u64 {
        let output = Command::new("cargo")
            .args(["build", "--release"])
            .current_dir(&self.project_path)
            .env("CARGO_TERM_COLOR", "never")
            .output();

        match output {
            Ok(out) if out.status.success() => {
                // Find the .rlib or .so in target/release/deps/
                let deps_dir = self.project_path.join("target/release/deps");
                find_largest_artifact(&deps_dir)
            }
            _ => {
                warn!("Release build failed, binary size = 0");
                0
            }
        }
    }

    /// Run the full evaluation pipeline: build → test → measure.
    pub fn evaluate(&self, scenario: &Scenario) -> Result<Report, crate::GymError> {
        self.inject(scenario)?;

        // Step 1: Build
        let (compilation, compile_time_ms) = self.build(scenario.timeout_secs)?;

        if !compilation.compiled {
            return Ok(Report {
                success: false,
                compilation,
                tests: None,
                fuzz: None,
                adversarial: None,
                metrics: Metrics {
                    compile_time_ms,
                    binary_size_bytes: 0,
                    test_time_ms: 0,
                },
                raw_stderr: String::new(),
            });
        }

        // Step 2: Tests (if provided or if fuzz tests were generated)
        let has_tests = !scenario.test_code.is_empty() || scenario.fuzz;
        let (tests, test_time_ms) = if has_tests {
            let (result, ms) = self.test(scenario.timeout_secs)?;
            (Some(result), ms)
        } else {
            (None, 0)
        };

        // Step 2b: Parse fuzz results from test output
        let fuzz = if scenario.fuzz {
            tests.as_ref().map(|t| parse_fuzz_results(&t.output, &scenario.source_code))
        } else {
            None
        };

        // Step 3: Measure binary size (release build)
        let binary_size_bytes = self.measure_binary_size();

        // Step 4: Adversarial mutation testing (if enabled and tests exist)
        let adversarial = if scenario.adversarial && has_tests {
            let mutations = Saboteur::generate_mutations(&scenario.source_code);
            if mutations.is_empty() {
                None
            } else {
                let mut outcomes = Vec::new();
                let original_src = std::fs::read_to_string(self.project_path.join("src/lib.rs"))
                    .map_err(|e| crate::GymError::Sandbox(format!("Failed to read original source for restore: {e}")))?;

                for mutation in &mutations {
                    let mutated_source = Saboteur::apply_mutation(&scenario.source_code, mutation);

                    // Inject mutated source
                    let src_path = self.project_path.join("src/lib.rs");
                    if let Err(e) = std::fs::write(&src_path, &mutated_source) {
                        warn!(error = %e, "Gym: Failed to write mutated source");
                        continue;
                    }

                    // Quick check: does it compile?
                    let compiles = Command::new("cargo")
                        .args(["check", "--quiet"])
                        .current_dir(&self.project_path)
                        .output()
                        .map(|o| o.status.success())
                        .unwrap_or(false);

                    let detected = if compiles {
                        // Run tests — if they fail, mutation was detected
                        let test_output = Command::new("cargo")
                            .args(["test", "--quiet"])
                            .current_dir(&self.project_path)
                            .env("CARGO_TERM_COLOR", "never")
                            .output();
                        match test_output {
                            Ok(o) => !o.status.success(),
                            Err(_) => true, // error running = detected
                        }
                    } else {
                        true // compile error = mutation detected
                    };

                    outcomes.push(MutationOutcome {
                        mutation: mutation.clone(),
                        detected,
                    });
                }

                // Restore original source
                if let Err(e) = std::fs::write(self.project_path.join("src/lib.rs"), &original_src) {
                    warn!(error = %e, "Gym: Failed to restore original source after mutation testing");
                }

                let total = outcomes.len();
                let detected_count = outcomes.iter().filter(|o| o.detected).count();
                let survived = total - detected_count;
                let mutation_score = if total > 0 {
                    detected_count as f64 / total as f64
                } else {
                    1.0
                };

                Some(AdversarialResult {
                    total_mutations: total,
                    detected: detected_count,
                    survived,
                    mutation_score,
                    mutations: outcomes,
                })
            }
        } else {
            None
        };

        let success = compilation.compiled
            && tests.as_ref().is_none_or(|t| t.failed == 0);

        Ok(Report {
            success,
            compilation,
            tests,
            fuzz,
            adversarial,
            metrics: Metrics {
                compile_time_ms,
                binary_size_bytes,
                test_time_ms,
            },
            raw_stderr: String::new(),
        })
    }

    /// Path to the sandbox project root.
    pub fn path(&self) -> &Path {
        &self.project_path
    }

    // ── Private helpers ──────────────────────────────────────

    fn add_dependencies(&self, scenario: &Scenario, fuzz: bool) -> Result<(), crate::GymError> {
        let cargo_path = self.project_path.join("Cargo.toml");
        let mut content = std::fs::read_to_string(&cargo_path)
            .map_err(|e| crate::GymError::Sandbox(e.to_string()))?;

        // Add runtime dependencies
        if !scenario.dependencies.is_empty() {
            let mut deps_section = String::from("\n[dependencies]\n");
            for dep in &scenario.dependencies {
                if dep.features.is_empty() {
                    deps_section.push_str(&format!("{} = \"{}\"\n", dep.name, dep.version));
                } else {
                    deps_section.push_str(&format!(
                        "{} = {{ version = \"{}\", features = [{}] }}\n",
                        dep.name,
                        dep.version,
                        dep.features
                            .iter()
                            .map(|f| format!("\"{f}\""))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
            }

            // Replace existing [dependencies] or append
            if let Some(pos) = content.find("[dependencies]") {
                let section_end = content[pos + 14..]
                    .find("\n[")
                    .map(|p| pos + 14 + p)
                    .unwrap_or(content.len());
                content.replace_range(pos..section_end, &deps_section[1..]);
            } else {
                content.push_str(&deps_section);
            }
        }

        // Add proptest as dev-dependency for fuzz tests
        if fuzz {
            content.push_str("\n[dev-dependencies]\nproptest = \"1\"\n");
        }

        std::fs::write(&cargo_path, content)
            .map_err(|e| crate::GymError::Sandbox(e.to_string()))?;

        Ok(())
    }
}

/// Parse "test result: ok. X passed; Y failed; Z ignored; W measured; ..." from cargo test output.
/// Aggregates across all test binaries (lib + integration tests).
fn parse_test_summary(output: &str) -> (u32, u32, u32, u32) {
    let mut total_passed = 0u32;
    let mut total_failed = 0u32;
    let mut total_ignored = 0u32;
    let mut found_summary = false;

    for line in output.lines() {
        let line = line.trim();
        if line.starts_with("test result:") {
            found_summary = true;

            for part in line.split(';') {
                let part = part.trim();
                if let Some(n) = extract_number_before(part, "passed") {
                    total_passed += n;
                } else if let Some(n) = extract_number_before(part, "failed") {
                    total_failed += n;
                } else if let Some(n) = extract_number_before(part, "ignored") {
                    total_ignored += n;
                }
            }
        }
    }

    // Fallback: count individual "test <name> ... ok/FAILED" lines
    if !found_summary {
        for line in output.lines() {
            let line = line.trim();
            if line.starts_with("test ") && line.ends_with("... ok") {
                total_passed += 1;
            } else if line.starts_with("test ") && line.ends_with("... FAILED") {
                total_failed += 1;
            } else if line.starts_with("test ") && line.ends_with("... ignored") {
                total_ignored += 1;
            }
        }
    }

    let total = total_passed + total_failed + total_ignored;
    (total_passed, total_failed, total_ignored, total)
}

fn extract_number_before(s: &str, keyword: &str) -> Option<u32> {
    if s.contains(keyword) {
        s.split_whitespace()
            .find_map(|w| w.parse::<u32>().ok())
    } else {
        None
    }
}

/// Parse proptest fuzz results from cargo test output.
///
/// Counts fuzzed functions from the source, and extracts any proptest failures
/// by looking for the pattern: `thread 'fuzz_xxx' panicked` + `minimal failing input`.
fn parse_fuzz_results(test_output: &str, source: &str) -> FuzzResult {
    // Count how many fuzz_* functions were generated.
    let fuzzed_functions = fuzzer::generate_fuzz_tests(source)
        .map(|code| code.matches("fn fuzz_").count())
        .unwrap_or(0);

    let mut failures = Vec::new();

    // Parse proptest failures. Pattern:
    //   thread 'fuzz_xxx' panicked at 'Test failed: <message>'
    //   ...
    //   minimal failing input:
    //     <args>
    let lines: Vec<&str> = test_output.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();

        // Detect panicked fuzz test.
        if line.contains("panicked") && line.contains("fuzz_") {
            // Extract function name.
            let function = line
                .split('\'')
                .find(|s| s.starts_with("fuzz_"))
                .unwrap_or("unknown")
                .to_string();

            // Extract error message.
            let error = line
                .split("panicked at")
                .nth(1)
                .map(|s| s.trim().trim_matches('\'').trim_matches('"').to_string())
                .unwrap_or_else(|| line.to_string());

            // Look ahead for "minimal failing input:" or proptest counter-example.
            let mut failing_input = String::new();
            let mut j = i + 1;
            while j < lines.len() && j < i + 20 {
                let next = lines[j].trim();
                if next.starts_with("minimal failing input")
                    || next.starts_with("successes:")
                    || next.starts_with("test result:")
                {
                    // Collect input lines after "minimal failing input:".
                    if next.starts_with("minimal failing input") {
                        j += 1;
                        while j < lines.len() {
                            let input_line = lines[j].trim();
                            if input_line.is_empty()
                                || input_line.starts_with("successes:")
                                || input_line.starts_with("test result:")
                            {
                                break;
                            }
                            if !failing_input.is_empty() {
                                failing_input.push_str(", ");
                            }
                            failing_input.push_str(input_line);
                            j += 1;
                        }
                    }
                    break;
                }
                j += 1;
            }

            failures.push(FuzzFailure {
                function,
                failing_input,
                error,
            });
        }

        i += 1;
    }

    FuzzResult {
        fuzzed_functions,
        failures,
    }
}

fn find_largest_artifact(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };

    entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.contains("eval_project")
                && (name.ends_with(".rlib") || name.ends_with(".so") || name.ends_with(".dylib"))
        })
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .max()
        .unwrap_or(0)
}
