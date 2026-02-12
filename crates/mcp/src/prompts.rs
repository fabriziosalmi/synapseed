//! MCP Prompt definitions.
//!
//! Prompts are pre-built templates that guide the LLM to use
//! SYNAPSEED tools in optimal sequences.

use crate::protocol::{ContentBlock, PromptArgument, PromptDefinition, PromptMessage};

/// Return all available prompt definitions.
pub fn list_prompts() -> Vec<PromptDefinition> {
    vec![
        PromptDefinition {
            name: "describe_architecture".into(),
            description: "Analyze and describe the project architecture using SYNAPSEED's semantic understanding. Indexes the codebase, identifies key symbols, and produces a structured overview.".into(),
            arguments: Some(vec![
                PromptArgument {
                    name: "path".into(),
                    description: "Directory to analyze (default: project root)".into(),
                    required: false,
                },
                PromptArgument {
                    name: "depth".into(),
                    description: "Analysis depth: 'overview' (files + top-level), 'detailed' (all symbols + relationships), 'deep' (includes git history)".into(),
                    required: false,
                },
            ]),
        },
        PromptDefinition {
            name: "visualize_architecture".into(),
            description: "Open the live architecture dashboard and guide the user through the interactive visualization. Indexes the project, launches the visualizer, and explains the graph.".into(),
            arguments: Some(vec![
                PromptArgument {
                    name: "focus".into(),
                    description: "Optional focus area: a file path, module name, or symbol to highlight in the graph".into(),
                    required: false,
                },
            ]),
        },
        PromptDefinition {
            name: "fix_build_errors".into(),
            description: "Diagnose and fix current build errors using the shadow compiler. Reads live diagnostics and applies compiler-suggested fixes automatically.".into(),
            arguments: Some(vec![
                PromptArgument {
                    name: "auto_fix".into(),
                    description: "If 'true', automatically apply all MachineApplicable fixes. If 'false' (default), show diagnostics and ask before fixing.".into(),
                    required: false,
                },
            ]),
        },
        PromptDefinition {
            name: "explain_evolution".into(),
            description: "Analyze why a piece of code looks the way it does by tracing its evolution through git history. Uses churn analysis, commit semantics, and co-change patterns to explain complexity.".into(),
            arguments: Some(vec![
                PromptArgument {
                    name: "file".into(),
                    description: "File path to analyze".into(),
                    required: true,
                },
                PromptArgument {
                    name: "start_line".into(),
                    description: "Start line of the region of interest".into(),
                    required: false,
                },
                PromptArgument {
                    name: "end_line".into(),
                    description: "End line of the region of interest".into(),
                    required: false,
                },
            ]),
        },
        PromptDefinition {
            name: "security_audit".into(),
            description: "Perform a comprehensive security audit of the project. Scans for secrets, evaluates command policies, checks git history for leaked credentials, and produces a security report.".into(),
            arguments: Some(vec![
                PromptArgument {
                    name: "scope".into(),
                    description: "Audit scope: 'quick' (DLP scan only), 'standard' (DLP + command policy), 'full' (DLP + commands + git history + file permissions)".into(),
                    required: false,
                },
            ]),
        },
        PromptDefinition {
            name: "optimize_hotspots".into(),
            description: "Analyze runtime performance hotspots from OTLP telemetry data. Identifies the slowest code paths, correlates with git history and code structure, and suggests targeted optimizations.".into(),
            arguments: Some(vec![
                PromptArgument {
                    name: "threshold".into(),
                    description: "Minimum average duration in ms to flag as a hotspot (default: 100)".into(),
                    required: false,
                },
            ]),
        },
    ]
}

/// Expand a prompt into messages for the LLM.
pub fn get_prompt(name: &str, args: &serde_json::Value) -> Option<Vec<PromptMessage>> {
    match name {
        "describe_architecture" => Some(prompt_describe_architecture(args)),
        "visualize_architecture" => Some(prompt_visualize_architecture(args)),
        "fix_build_errors" => Some(prompt_fix_build_errors(args)),
        "explain_evolution" => Some(prompt_explain_evolution(args)),
        "security_audit" => Some(prompt_security_audit(args)),
        "optimize_hotspots" => Some(prompt_optimize_hotspots(args)),
        _ => None,
    }
}

fn prompt_describe_architecture(args: &serde_json::Value) -> Vec<PromptMessage> {
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
    let depth = args
        .get("depth")
        .and_then(|v| v.as_str())
        .unwrap_or("detailed");

    let instructions = match depth {
        "overview" => format!(
            r#"Analyze the project architecture at "{path}". Follow these steps:

1. Use `get_code_skeleton` to index the project directory at "{path}"
2. Read `synapseed://status` to understand the project state
3. Read `synapseed://dna` for configuration context

Produce a concise architectural overview:
- Project type and build system
- Top-level directory structure
- Key entry points
- Number of files and symbols indexed"#
        ),
        "deep" => format!(
            r#"Perform a deep architectural analysis of "{path}". Follow these steps:

1. Use `get_code_skeleton` to index the project at "{path}"
2. Read `synapseed://status` for project state and metrics
3. Read `synapseed://dna` for configuration context
4. Use `git_history` on key files (entry points, config files) to understand evolution
5. Use `lookup_symbol` for any critical types, traits, or interfaces found in the skeleton

Produce a comprehensive architecture document:
- Project type, build system, and workspace layout
- Module dependency graph (which modules depend on which)
- Key abstractions (traits, interfaces, base classes)
- Historical evolution: how the architecture grew over time
- Potential architectural concerns or technical debt
- Recommendations for improvement"#
        ),
        _ => format!(
            r#"Analyze the project architecture at "{path}" in detail. Follow these steps:

1. Use `get_code_skeleton` to index the project directory at "{path}"
2. Read `synapseed://status` to understand the project state
3. Read `synapseed://dna` for configuration context
4. Use `lookup_symbol` to find key types: look for "main", "app", "server", "config", "router", "handler"

Produce a detailed architectural overview:
- Project type, build system, and workspace layout
- Directory structure with purpose of each module
- Key symbols and their roles (entry points, core types, traits/interfaces)
- Data flow: how information moves through the system
- Dependencies: external crates/packages used and why"#
        ),
    };

    vec![PromptMessage {
        role: "user".into(),
        content: ContentBlock::Text { text: instructions },
    }]
}

fn prompt_visualize_architecture(args: &serde_json::Value) -> Vec<PromptMessage> {
    let focus = args.get("focus").and_then(|v| v.as_str()).unwrap_or("");

    let focus_instruction = if focus.is_empty() {
        String::new()
    } else {
        format!("\n5. Look for the node matching \"{focus}\" in the graph and guide the user to it")
    };

    let instructions = format!(
        r#"Help the user visualize their project architecture. Follow these steps:

1. Read `synapseed://visualizer/url` to get the dashboard URL
2. Use `get_code_skeleton` to index the project and ensure the graph data is ready
3. Tell the user to open the dashboard URL in their browser
4. Describe what they'll see: an interactive Cytoscape.js graph with file nodes (dark containers) containing symbol nodes (colored by type: green=functions, cyan=methods, blue=structs, purple=enums, orange=modules){focus_instruction}

Explain the dashboard features:
- **Zoom/Pan**: Scroll to zoom, drag to pan
- **Hover**: Hover over symbol nodes to see name, kind, and line numbers
- **Live Updates**: Edit any source file and watch the corresponding node pulse orange
- **Controls**: Refresh (re-index), Fit (center graph), Activity (show change log)

Color legend:
- Green: Functions
- Cyan: Methods
- Blue: Structs / Classes / Interfaces
- Purple: Enums
- Orange: Modules / Constants
- Yellow: Variables
- Gray: Imports"#
    );

    vec![PromptMessage {
        role: "user".into(),
        content: ContentBlock::Text { text: instructions },
    }]
}

fn prompt_fix_build_errors(args: &serde_json::Value) -> Vec<PromptMessage> {
    let auto_fix = args
        .get("auto_fix")
        .and_then(|v| v.as_str())
        .unwrap_or("false")
        == "true";

    let fix_instruction = if auto_fix {
        "4. For each error/warning that has a `MachineApplicable` suggestion, automatically use `apply_quick_fix` to apply it\n5. After applying all fixes, run `get_diagnostics` again to verify the fixes worked"
    } else {
        "4. For each error/warning, explain what it means and whether a compiler-suggested fix is available\n5. Ask the user if they want to apply the available fixes\n6. If approved, use `apply_quick_fix` for each fixable issue"
    };

    let instructions = format!(
        r#"Help the user fix current build errors using the shadow compiler. Follow these steps:

1. Read `synapseed://diagnostics/active` to see the current compiler state
2. Use `get_diagnostics` to get the full list of errors and warnings
3. Group diagnostics by file and severity (errors first, then warnings)

{fix_instruction}

Report format:
- For each diagnostic: file:line — [ERROR/WARNING] code — message
- If a suggestion is available: "FIX AVAILABLE: description"
- Final summary: X errors fixed, Y remaining, Z warnings

Important:
- Only apply `MachineApplicable` suggestions automatically
- For `MaybeIncorrect` or `HasPlaceholders` suggestions, always ask the user first
- After fixing, the shadow compiler will automatically re-check (wait ~2 seconds)"#
    );

    vec![PromptMessage {
        role: "user".into(),
        content: ContentBlock::Text { text: instructions },
    }]
}

fn prompt_explain_evolution(args: &serde_json::Value) -> Vec<PromptMessage> {
    let file = args
        .get("file")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let start_line = args
        .get("start_line")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let end_line = args.get("end_line").and_then(|v| v.as_str()).unwrap_or("");

    let line_range_instruction = if !start_line.is_empty() && !end_line.is_empty() {
        format!(
            r#"3. Use `analyze_history` with file="{file}", start_line={start_line}, end_line={end_line}
4. Use `git_history` with file="{file}", start_line={start_line}, end_line={end_line} for detailed blame"#
        )
    } else if !start_line.is_empty() {
        format!(
            r#"3. Use `analyze_history` with file="{file}", start_line={start_line}
4. Use `git_history` with file="{file}", start_line={start_line} for detailed blame"#
        )
    } else {
        format!(
            r#"3. Use `analyze_history` with file="{file}" for full-file analysis
4. Use `git_history` with file="{file}" for detailed blame on key areas"#
        )
    };

    let instructions = format!(
        r#"Explain why this code looks the way it does by analyzing its evolution through git history.

Follow these steps:

1. Use `get_code_skeleton` to understand the project structure
2. Use `lookup_symbol` to find the symbols in "{file}" and understand their role

{line_range_instruction}

5. Analyze the results and produce a narrative explanation:

--- Structure your response as: ---

## Evolution of {file}

### Timeline
- List the key commits chronologically, highlighting what changed and why
- Tag each commit: [FIX], [FEATURE], [REFACTOR], [SECURITY], etc.

### Why It Looks This Way
- Explain the accumulated complexity: what problems were solved, what trade-offs were made
- Connect patterns: "The function was rewritten N times because..."
- Identify if complexity comes from fixes (reactive) or features (intentional)

### Risk Assessment
- Hotspot score interpretation (is this a volatile area?)
- Co-change patterns (what other files are coupled to this?)
- Regression risk based on revert/fix frequency

### Recommendations
- Is a refactor warranted based on the churn data?
- Are there co-change dependencies that should be decoupled?
- Should tests be added based on the fix frequency?

Important: Base your analysis strictly on the data from `analyze_history` and `git_history`. Do not speculate beyond what the commit data shows."#
    );

    vec![PromptMessage {
        role: "user".into(),
        content: ContentBlock::Text { text: instructions },
    }]
}

fn prompt_optimize_hotspots(args: &serde_json::Value) -> Vec<PromptMessage> {
    let threshold = args
        .get("threshold")
        .and_then(|v| v.as_str())
        .unwrap_or("100");

    let instructions = format!(
        r#"Analyze runtime performance hotspots and suggest optimizations. Follow these steps:

1. Read `synapseed://telemetry/hotspots` to get the current performance data
2. Filter hotspots with avg_duration_ms >= {threshold}ms
3. For each hotspot above the threshold:
   a. Use `lookup_symbol` to find the function/method in the codebase
   b. Use `analyze_history` on the file to understand churn and complexity
   c. Use `get_diagnostics` to check if there are compiler warnings in that area
4. Read `synapseed://status` for overall project health context

Produce a performance optimization report:

## Runtime Hotspots (threshold: {threshold}ms)

For each hotspot:
- **Location**: file:line — symbol name
- **Metrics**: avg, max, p95, call count
- **Classification**: CPU-bound / IO-bound / Memory / Unknown
- **History**: churn score, recent changes, fix frequency
- **Recommendation**: specific optimization suggestion

## Summary
- Total hotspots above threshold
- Estimated impact of fixing top-3 hotspots
- Priority order for optimization

## Action Plan
- Ordered list of specific code changes to make
- Expected improvement for each change
- Risk assessment (could the change introduce regressions?)

Important:
- Base analysis on actual telemetry data, not speculation
- If no telemetry data is available, instruct the user to configure their app's OTLP exporter to send traces to 127.0.0.1:4317
- Consider both latency AND call frequency when prioritizing"#
    );

    vec![PromptMessage {
        role: "user".into(),
        content: ContentBlock::Text { text: instructions },
    }]
}

fn prompt_security_audit(args: &serde_json::Value) -> Vec<PromptMessage> {
    let scope = args
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("standard");

    let instructions = match scope {
        "quick" => r#"Perform a quick security scan of this project. Steps:

1. Read `synapseed://security/policy` to understand active rules
2. Read `synapseed://status` for project state
3. Use `get_code_skeleton` to find all source files
4. For each configuration file found (*.toml, *.yaml, *.json, *.env), use `scan_security` to check for leaked secrets

Produce a security report:
- Number of files scanned
- Findings (if any): file, line, type of secret
- Overall risk assessment: CLEAN / LOW / MEDIUM / HIGH / CRITICAL"#
            .into(),
        "full" => r#"Perform a comprehensive security audit. Steps:

1. Read `synapseed://security/policy` to understand all active rules
2. Read `synapseed://status` for project state and metrics
3. Read `synapseed://dna` for DLP level configuration
4. Use `get_code_skeleton` to index the full project
5. For each source and config file, use `scan_security` to check content
6. Use `check_command` to verify common commands used in the project (check CI scripts, Makefiles, package.json scripts)
7. Use `git_history` on sensitive files (.env, config files, key files) to check if secrets were ever committed
8. Use `project_diagnose` for overall health check

Produce a comprehensive security report:
- Executive summary
- DLP Findings: secrets, credentials, PII in current code
- Command Policy: dangerous commands in scripts or configs
- Git History: any secrets in commit history (even if removed now)
- Configuration: DLP level appropriateness, policy gaps
- Risk Matrix: categorize all findings by severity
- Remediation: specific steps to fix each finding
- Overall risk assessment: CLEAN / LOW / MEDIUM / HIGH / CRITICAL"#
            .into(),
        _ => r#"Perform a standard security audit of this project. Steps:

1. Read `synapseed://security/policy` to understand active rules
2. Read `synapseed://status` for project state
3. Use `get_code_skeleton` to index the project
4. For each source and config file, use `scan_security` to check for secrets
5. Use `check_command` to verify safety of common project commands
6. Use `project_diagnose` for overall health check

Produce a security report:
- DLP Scan Results: files scanned, secrets found (if any)
- Command Policy: evaluation of common commands
- Project Health: overall diagnostic state
- Risk Assessment: CLEAN / LOW / MEDIUM / HIGH / CRITICAL
- Recommendations: specific remediation steps for any findings"#
            .into(),
    };

    vec![PromptMessage {
        role: "user".into(),
        content: ContentBlock::Text { text: instructions },
    }]
}
