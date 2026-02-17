//! Raw source injection — reads and minifies actual source code for LLM consumption.
//!
//! Contains all budget management, noise pruning, and source preparation logic
//! for the "Direct Symbol Injection" feature. Split from context_builder (#64).

use tracing::debug;

use synapseed_core::context::SynapseContext;
use synapseed_core::momentum::ModelTier;
use synapseed_core::pulse::{PulseStore, COUNTER_FILE_TOUCHED};
use synapseed_cortex::graph::CodeGraph;
use synapseed_husk::patterns::sanitize_prompt_tokens;

use crate::router::{RawSource, Target};

// ── Raw Source Injection ────────────────────────────────────────────────

/// Minify source code to reduce token waste without losing semantics.
///
/// - Strips trailing whitespace from each line
/// - Collapses runs of 3+ blank lines into a single blank line
pub(super) fn minify_source(source: &str) -> String {
    let mut result = Vec::new();
    let mut blank_run = 0;

    for line in source.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            blank_run += 1;
            if blank_run <= 1 {
                result.push("");
            }
            // Runs of 2+ blanks → collapsed to 1
        } else {
            blank_run = 0;
            result.push(trimmed);
        }
    }

    result.join("\n")
}

/// Prune non-structural noise from source code (v4.7.0 — "La Dieta del Token").
///
/// Collapses logging/debug statements into a single comment and truncates
/// overly long lines (string constants, generated code). Preserves all structural
/// code (function signatures, control flow, return values) intact.
///
/// Savings: typically 10-30% token reduction on real-world code.
pub(super) fn prune_noise(source: &str, file_ext: &str) -> String {
    let comment = match file_ext {
        "py" | "pyi" | "rb" | "sh" | "bash" | "yaml" | "yml" | "toml" => "#",
        _ => "//",
    };

    let mut result = Vec::new();
    let mut in_log_block = false;
    let mut paren_depth: i32 = 0;
    let mut last_was_pruned = false;

    for line in source.lines() {
        let trimmed = line.trim();

        // Track multi-line log blocks (e.g., debug!(\n  field = value,\n  "msg"\n);)
        if in_log_block {
            paren_depth += trimmed.chars().filter(|&c| c == '(').count() as i32;
            paren_depth -= trimmed.chars().filter(|&c| c == ')').count() as i32;
            if paren_depth <= 0 {
                in_log_block = false;
                paren_depth = 0;
            }
            continue; // swallow line
        }

        // Detect logging statements
        if is_log_statement(trimmed) {
            paren_depth = trimmed.chars().filter(|&c| c == '(').count() as i32
                - trimmed.chars().filter(|&c| c == ')').count() as i32;
            if paren_depth > 0 {
                in_log_block = true;
            }
            if !last_was_pruned {
                let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
                result.push(format!("{indent}{comment} ..."));
            }
            last_was_pruned = true;
            continue;
        }

        last_was_pruned = false;

        // Truncate overly long lines (string constants, generated code)
        if line.len() > 200 {
            // Safe truncation: find nearest char boundary before 200
            let end = line.floor_char_boundary(200);
            result.push(format!("{}...", &line[..end]));
        } else {
            result.push(line.to_string());
        }
    }

    result.join("\n")
}

/// Detect logging/debug statements across common languages.
///
/// Covers Rust (tracing, log, std), Python (logging, logger, print),
/// and JavaScript/TypeScript (console.*).
pub(super) fn is_log_statement(trimmed: &str) -> bool {
    // Rust: macro-based logging
    for prefix in [
        "debug!(", "info!(", "warn!(", "error!(", "trace!(",
        "println!(", "eprintln!(", "dbg!(",
        "tracing::debug!(", "tracing::info!(", "tracing::warn!(",
        "tracing::error!(", "tracing::trace!(",
        "log::debug!(", "log::info!(", "log::warn!(",
        "log::error!(", "log::trace!(",
    ] {
        if trimmed.starts_with(prefix) {
            return true;
        }
    }

    // Python: logger.method() / logging.method()
    for obj in ["logger.", "logging.", "log."] {
        if trimmed.starts_with(obj) {
            let after = &trimmed[obj.len()..];
            for method in ["debug(", "info(", "warning(", "error(", "critical(", "exception("] {
                if after.starts_with(method) {
                    return true;
                }
            }
        }
    }

    // Python: print() — almost always debug noise in framework code
    if trimmed.starts_with("print(") {
        return true;
    }

    // JavaScript/TypeScript: console.*()
    if trimmed.starts_with("console.") {
        let after = &trimmed["console.".len()..];
        for method in ["log(", "error(", "warn(", "debug(", "info(", "trace("] {
            if after.starts_with(method) {
                return true;
            }
        }
    }

    false
}

/// Inject raw source code for each target, respecting the tier's budget.
///
/// Budgets are derived from `ModelTier::source_char_budget()` (Context
/// Budgeting v5.0).  Critical symbols get a dedicated pool from
/// `ModelTier::critical_char_budget()` so they don't starve the shared pool.
pub(in crate::router) fn inject_raw_sources(targets: &[Target], ctx: &SynapseContext, tier: ModelTier) -> Vec<RawSource> {
    let char_budget: usize = tier.source_char_budget();
    let critical_budget: usize = tier.critical_char_budget();
    let min_lines: usize = tier.min_source_lines();
    let root = ctx.project_root();
    let critical_symbols = ctx.dna().context.critical_symbols;
    let mut sources = Vec::new();
    let mut budget_used: usize = 0;
    let mut critical_used: usize = 0;

    // Sort targets by score DESC so the most relevant symbols get budget priority.
    // Critical symbols (from DNA config) are always processed first regardless of score.
    // Pulse boost (v4.23.0): files from recent activity get a tiebreaker bonus.
    let pulse = ctx.get_extension::<PulseStore>();
    let mut sorted_targets: Vec<&Target> = targets.iter().collect();
    sorted_targets.sort_by(|a, b| {
        let a_critical = critical_symbols.iter().any(|c| a.name.contains(c.as_str()));
        let b_critical = critical_symbols.iter().any(|c| b.name.contains(c.as_str()));
        match (a_critical, b_critical) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => {
                let sa = a.score.unwrap_or(0.0);
                let sb = b.score.unwrap_or(0.0);
                // Pulse tiebreaker: at equal search score, prefer files
                // the user has been working with recently.
                let pa = pulse.as_ref().map_or(0.0, |p| {
                    a.file_path.as_deref().map_or(0.0, |fp| p.score_of(COUNTER_FILE_TOUCHED, fp) as f32)
                });
                let pb = pulse.as_ref().map_or(0.0, |p| {
                    b.file_path.as_deref().map_or(0.0, |fp| p.score_of(COUNTER_FILE_TOUCHED, fp) as f32)
                });
                // Blend: search score + configurable pulse bonus (#67)
                let pw = ctx.dna().context.pulse_blend_weight;
                let blended_a = sa + pa * pw;
                let blended_b = sb + pb * pw;
                blended_b.total_cmp(&blended_a)
            }
        }
    });

    // Retrieve the code graph from the context for precise line range lookup
    let graph = ctx.get_extension::<CodeGraph>();

    for target in &sorted_targets {
        let rel_path = match &target.file_path {
            Some(p) => p.clone(),
            None => continue,
        };

        let abs_path = root.join(&rel_path);
        let content = match std::fs::read_to_string(&abs_path) {
            Ok(c) => c,
            Err(e) => {
                // v5.1 (D10): silently skip — stale targets should have
                // been filtered in Stage 3.5, but as defense-in-depth we
                // just drop them here rather than injecting an error marker
                // that would confuse the LLM.
                debug!(path = %abs_path.display(), error = %e, "Whisper: skipping unreadable file for raw injection");
                continue;
            }
        };

        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            continue;
        }

        // Try to get precise line range from Cortex (Symbol ID or Name lookup)
        let (start, end) = if let Some(g) = &graph {
            let candidates = g.lookup(&target.name);
            let sym = candidates.iter().find(|s| s.file_path.ends_with(&rel_path));

            if let Some(s) = sym {
                // Enum/Constant expansion (v4.15.0): enums and constants often
                // have their value spread over many lines after the definition.
                // Expand the window so the model sees all variants/values.
                let extra = match s.kind {
                    synapseed_core::symbol::SymbolKind::Enum => 25,
                    synapseed_core::symbol::SymbolKind::Constant => 15,
                    _ => 0,
                };
                (s.line_start, (s.line_end + extra).min(lines.len()))
            } else if let Some(ls) = target.line_start {
                let s = ls.saturating_sub(1);
                let e = (ls + 30).min(lines.len());
                (s + 1, e)
            } else {
                (1, lines.len().min(60))
            }
        } else if let Some(ls) = target.line_start {
            // Fallback if no graph: ±30 lines around the target
            let s = ls.saturating_sub(1);
            let e = (ls + 30).min(lines.len());
            (s + 1, e) // 1-indexed
        } else {
            // No line info and no graph — take first 60 lines
            (1, lines.len().min(60))
        };

        // Clamp to file bounds (1-indexed)
        let s = start.max(1).min(lines.len());
        let mut e = end.max(s).min(lines.len());

        // Semantic Ballast: ensure at least min_lines per snippet
        if min_lines > 0 && (e - s + 1) < min_lines {
            e = (s + min_lines - 1).min(lines.len());
        }

        let snippet: String = lines[(s - 1)..e].join("\n");
        let snippet = minify_source(&snippet);

        // AST-based noise reduction (v4.7.0): prune logging, debug output,
        // and overly long lines to maximize context efficiency.
        let file_ext = rel_path.rsplit('.').next().unwrap_or("");
        let snippet = prune_noise(&snippet, file_ext);

        // D23 fix: neutralize prompt injection markers (LLM control tokens,
        // social-engineering override phrases) BEFORE sending to the model.
        // Detection existed in CodePatternScanner; this closes the pipeline gap.
        let snippet = sanitize_prompt_tokens(&snippet);

        // Budget management: truncate oversized snippets instead of skipping.
        // For very large functions, a truncated view is better than nothing.
        // Critical symbols use a DEDICATED budget pool (v4.19.1) so they don't
        // compete with or starve normal symbols. Normal symbols use the shared pool.
        let is_critical = critical_symbols.iter().any(|c| target.name.contains(c.as_str()));
        let snippet = if is_critical {
            if critical_used + snippet.len() > critical_budget {
                // Even critical symbols have a ceiling to prevent one massive
                // function from consuming all context. Smart-truncate within
                // the critical budget.
                let remaining = critical_budget.saturating_sub(critical_used);
                if remaining < 200 {
                    debug!(name = %target.name, "Whisper: critical budget exhausted, skipping");
                    continue;
                }
                let end = snippet.floor_char_boundary(remaining);
                debug!(name = %target.name, original = snippet.len(), truncated = end, "Whisper: critical symbol truncated to dedicated budget");
                snippet[..end].to_string()
            } else {
                debug!(name = %target.name, len = snippet.len(), "Whisper: critical symbol — dedicated budget");
                snippet
            }
        } else if budget_used + snippet.len() > char_budget {
            let remaining = char_budget.saturating_sub(budget_used);
            if remaining < 200 {
                // Budget nearly exhausted — skip remaining targets
                continue;
            }
            // Smart truncation: keep first half + last quarter of the budget slice
            // so the model sees both the function header and its tail.
            let first_portion = remaining * 3 / 4;
            let last_portion = remaining - first_portion - 30; // 30 chars for separator
            if last_portion > 50 && snippet.len() > remaining {
                let first_end = snippet.floor_char_boundary(first_portion);
                // D28: AST-aware cut — walk backward from first_end to find a
                // closing brace or newline boundary so we don't sever a statement
                // mid-expression. Limit scan to 200 chars to avoid excessive retreat.
                let first_end = {
                    let search_start = first_end.saturating_sub(200);
                    let window = &snippet[search_start..first_end];
                    // Prefer closing brace on its own line (end of block)
                    if let Some(pos) = window.rfind("\n}") {
                        search_start + pos + 2 // include the `}`
                    } else if let Some(pos) = window.rfind(";\n") {
                        search_start + pos + 2 // include the `;\n`
                    } else {
                        first_end // fallback to char boundary
                    }
                };
                let last_start = snippet.floor_char_boundary(snippet.len() - last_portion);
                // D28: AST-aware cut — walk forward from last_start to find the
                // start of a line or opening statement.
                let last_start = {
                    let search_end = (last_start + 200).min(snippet.len());
                    let window = &snippet[last_start..search_end];
                    if let Some(pos) = window.find("\nfn ") {
                        last_start + pos + 1
                    } else if let Some(pos) = window.find("\npub ") {
                        last_start + pos + 1
                    } else if let Some(pos) = window.find('\n') {
                        last_start + pos + 1 // at least align to line boundary
                    } else {
                        last_start
                    }
                };
                let mut truncated = snippet[..first_end].to_string();
                truncated.push_str("\n// ... [truncated] ...\n");
                truncated.push_str(&snippet[last_start..]);
                debug!(file = %rel_path, original = snippet.len(), truncated = truncated.len(), "Whisper: truncated oversized snippet");
                truncated
            } else {
                // Just take what fits
                let end = snippet.floor_char_boundary(remaining);
                snippet[..end].to_string()
            }
        } else {
            snippet
        };
        // Route budget accounting to the appropriate pool
        if is_critical {
            critical_used += snippet.len();
        } else {
            budget_used += snippet.len();
        }

        sources.push(RawSource {
            file_path: rel_path,
            line_start: s,
            line_end: e,
            source: snippet,
        });
    }

    sources
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Noise Reduction tests (v4.7.0 — "La Dieta del Token") ───────

    #[test]
    fn test_prune_rust_logging_single_line() {
        let source = r#"fn calculate(x: i32) -> i32 {
    debug!("Starting calculation with x={}", x);
    let result = x * 2;
    info!("Result: {}", result);
    result
}"#;
        let pruned = prune_noise(source, "rs");
        assert!(pruned.contains("fn calculate"));
        assert!(pruned.contains("let result = x * 2;"));
        assert!(pruned.contains("result"));
        assert!(!pruned.contains("debug!"));
        assert!(!pruned.contains("info!"));
        assert!(pruned.contains("// ..."));
    }

    #[test]
    fn test_prune_rust_multiline_logging() {
        let source = r#"fn process() {
    debug!(
        cs = cs,
        threshold = COHERENCE_THRESHOLD,
        max_clusters,
        "Coherence Gate: TRIGGERED"
    );
    let x = 42;
}"#;
        let pruned = prune_noise(source, "rs");
        assert!(pruned.contains("fn process()"));
        assert!(pruned.contains("let x = 42;"));
        assert!(!pruned.contains("cs = cs"));
        assert!(!pruned.contains("TRIGGERED"));
        assert!(pruned.contains("// ..."));
    }

    #[test]
    fn test_prune_python_logging() {
        let source = r#"def complex_calc(x):
    logger.info(f"Starting calculation with {x}")
    if x < 0:
        raise ValueError("negative")
    logging.debug("intermediate step")
    return x * 2"#;
        let pruned = prune_noise(source, "py");
        assert!(pruned.contains("def complex_calc"));
        assert!(pruned.contains("raise ValueError"));
        assert!(pruned.contains("return x * 2"));
        assert!(!pruned.contains("logger.info"));
        assert!(!pruned.contains("logging.debug"));
        assert!(pruned.contains("# ..."));
    }

    #[test]
    fn test_prune_javascript_console() {
        let source = r#"function handleRequest(req) {
    console.log("Received request:", req.url);
    const result = processRequest(req);
    console.error("Error:", result.error);
    return result;
}"#;
        let pruned = prune_noise(source, "js");
        assert!(pruned.contains("function handleRequest"));
        assert!(pruned.contains("const result = processRequest(req);"));
        assert!(pruned.contains("return result;"));
        assert!(!pruned.contains("console.log"));
        assert!(!pruned.contains("console.error"));
    }

    #[test]
    fn test_prune_consecutive_logs_single_marker() {
        let source = r#"fn init() {
    info!("Starting...");
    info!("Loading config...");
    info!("Connecting...");
    let db = connect();
}"#;
        let pruned = prune_noise(source, "rs");
        // 3 consecutive logs → only 1 "// ..." marker
        let marker_count = pruned.matches("// ...").count();
        assert_eq!(marker_count, 1, "Should collapse consecutive logs: {pruned}");
        assert!(pruned.contains("let db = connect();"));
    }

    #[test]
    fn test_prune_preserves_structural_code() {
        let source = r#"pub fn authenticate(creds: &Credentials) -> Result<Token, AuthError> {
    let user = find_user(&creds.username)?;
    if !verify_password(&user, &creds.password) {
        return Err(AuthError::InvalidPassword);
    }
    Ok(generate_token(&user))
}"#;
        let pruned = prune_noise(source, "rs");
        // No logging → no changes
        assert_eq!(pruned, source);
    }

    #[test]
    fn test_prune_long_line_truncation() {
        let long_line = format!("let msg = \"{}\";", "x".repeat(250));
        let source = format!("fn foo() {{\n    {long_line}\n    return 42;\n}}");
        let pruned = prune_noise(&source, "rs");
        assert!(pruned.contains("return 42;"));
        // The long line should be truncated
        assert!(!pruned.contains(&"x".repeat(250)));
        assert!(pruned.contains("..."));
    }

    #[test]
    fn test_prune_rust_println_and_dbg() {
        let source = r#"fn main() {
    println!("Debug output: {:?}", data);
    dbg!(&value);
    eprintln!("Warning: {}", msg);
    do_work();
}"#;
        let pruned = prune_noise(source, "rs");
        assert!(!pruned.contains("println!"));
        assert!(!pruned.contains("dbg!"));
        assert!(!pruned.contains("eprintln!"));
        assert!(pruned.contains("do_work();"));
    }

    #[test]
    fn test_prune_python_print() {
        let source = r#"def process(data):
    print(f"Processing {len(data)} items")
    result = transform(data)
    print("Done")
    return result"#;
        let pruned = prune_noise(source, "py");
        assert!(!pruned.contains("print("));
        assert!(pruned.contains("result = transform(data)"));
        assert!(pruned.contains("return result"));
    }

    #[test]
    fn test_is_log_statement_coverage() {
        // Rust
        assert!(is_log_statement("debug!(\"msg\");"));
        assert!(is_log_statement("tracing::info!(key = val, \"msg\");"));
        assert!(is_log_statement("log::warn!(\"msg\");"));
        assert!(is_log_statement("println!(\"hello\");"));
        assert!(is_log_statement("dbg!(value);"));

        // Python
        assert!(is_log_statement("logger.debug(\"msg\")"));
        assert!(is_log_statement("logging.error(\"msg\")"));
        assert!(is_log_statement("print(\"debug output\")"));

        // JavaScript
        assert!(is_log_statement("console.log(\"msg\");"));
        assert!(is_log_statement("console.error(\"msg\");"));

        // NOT logging
        assert!(!is_log_statement("let debug = true;"));
        assert!(!is_log_statement("fn info() {}"));
        assert!(!is_log_statement("logger_factory.create()"));
        assert!(!is_log_statement("result = console_app.run()"));
        assert!(!is_log_statement("return x * 2;"));
    }
}
