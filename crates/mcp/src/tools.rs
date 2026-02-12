//! MCP Tool definitions and handlers.
//!
//! Each tool maps to an internal SYNAPSEED capability.

use std::path::Path;

use serde_json::json;
use tracing::info;

use synapseed_architect::ReportStore;
use synapseed_chronos::historian::Historian;
use synapseed_core::context::SynapseContext;
use synapseed_core::state::ProjectState;
use synapseed_cortex::graph::CodeGraph;
use synapseed_husk::guard::SecurityGuard;
use synapseed_root::sentinel::Sentinel;
use synapseed_search::indexer::SemanticIndex;
use synapseed_shadow_check::runner::DiagnosticStore;
use synapseed_gym::{Scenario, Trainer};
use synapseed_janitor::{Janitor, ProposalStore};
use synapseed_telemetry_sink::store::SpanStore;

use crate::protocol::{ContentBlock, ToolCallResult, ToolDefinition};

/// Return all available tool definitions.
pub fn list_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "get_code_skeleton".into(),
            description: "LOW-LEVEL — Index a project directory and return its AST skeleton (files, symbols, structure). Prefer `ask_synapseed` for holistic queries; use this only when you need raw symbol data.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to directory to index (default: project root)"
                    }
                }
            }),
        },
        ToolDefinition {
            name: "lookup_symbol".into(),
            description: "LOW-LEVEL — Find a symbol by name across the entire project. Returns file path, line numbers, and signature. Prefer `ask_synapseed` unless you know the exact symbol name.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Symbol name to search for"
                    }
                },
                "required": ["name"]
            }),
        },
        ToolDefinition {
            name: "scan_security".into(),
            description: "LOW-LEVEL — Scan text content for sensitive data (API keys, passwords, tokens, PII). Returns findings or CLEAN status. Called automatically by `ask_synapseed` when security-relevant.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "Text content to scan for secrets"
                    }
                },
                "required": ["content"]
            }),
        },
        ToolDefinition {
            name: "check_command".into(),
            description: "LOW-LEVEL — Evaluate a shell command against the security policy. Returns ALLOWED or DENIED with reason. Always call this before executing any shell command.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Shell command to evaluate"
                    }
                },
                "required": ["command"]
            }),
        },
        ToolDefinition {
            name: "git_history".into(),
            description: "LOW-LEVEL — Get git blame/history for a file. Shows who changed what and why. Prefer `analyze_history` for richer insights or `ask_synapseed` for holistic context.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file": {
                        "type": "string",
                        "description": "File path relative to project root"
                    },
                    "start_line": {
                        "type": "integer",
                        "description": "Start line (default: 1)"
                    },
                    "end_line": {
                        "type": "integer",
                        "description": "End line (default: 50)"
                    }
                },
                "required": ["file"]
            }),
        },
        ToolDefinition {
            name: "project_diagnose".into(),
            description: "LOW-LEVEL — Run a full diagnostic on the project: detect state (virgin/partial/healthy), build system, git status, active plugins. Included automatically in `ask_synapseed` responses.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDefinition {
            name: "consult_architect".into(),
            description: "LOW-LEVEL — Consult the project's architecture policy (DNA config). Returns preferred libraries, workspace strategy, naming conventions. Use `architect_analyze` for structural health or `ask_synapseed` for holistic answers.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Architecture question (e.g., 'which async runtime?', 'error handling strategy?', 'project layout?')"
                    }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "semantic_search".into(),
            description: "LOW-LEVEL — Search for code by concept (Tantivy keyword index). Finds symbols by name, signature, doc comments. Supports fuzzy matching. Prefer `ask_synapseed` for broad queries; use this for targeted symbol search.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Semantic search query (e.g., 'authentication login', 'error handling', 'database connection')"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results (default: 5)",
                        "default": 5
                    }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "get_diagnostics".into(),
            description: "LOW-LEVEL — Get current compiler diagnostics from the background shadow compiler. Optionally filter by file path and/or severity. Included automatically in `ask_synapseed` responses when relevant.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file": {
                        "type": "string",
                        "description": "Optional file path to filter diagnostics (returns all if omitted)"
                    },
                    "min_severity": {
                        "type": "string",
                        "enum": ["info", "warning", "error"],
                        "description": "Minimum severity to include (default: 'warning')",
                        "default": "warning"
                    }
                }
            }),
        },
        ToolDefinition {
            name: "analyze_history".into(),
            description: "LOW-LEVEL — Analyze file history: churn/hotspot score, co-change patterns, semantic commit classification, risk assessment. Use for deep dives into a specific file; `ask_synapseed` includes this automatically when relevant.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file": {
                        "type": "string",
                        "description": "File path relative to project root"
                    },
                    "start_line": {
                        "type": "integer",
                        "description": "Optional start line to scope analysis"
                    },
                    "end_line": {
                        "type": "integer",
                        "description": "Optional end line to scope analysis"
                    }
                },
                "required": ["file"]
            }),
        },
        ToolDefinition {
            name: "apply_quick_fix".into(),
            description: "LOW-LEVEL — Apply a compiler-suggested fix automatically. Only applies 'MachineApplicable' suggestions from rustc. Call `get_diagnostics` first to find the error code.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file": {
                        "type": "string",
                        "description": "File path containing the error"
                    },
                    "error_code": {
                        "type": "string",
                        "description": "The error/warning code to fix (e.g., 'unused_variables', 'E0425')"
                    }
                },
                "required": ["file", "error_code"]
            }),
        },
        ToolDefinition {
            name: "ask_synapseed".into(),
            description: "PRIMARY TOOL — Start here. Ask a natural-language question and SYNAPSEED automatically orchestrates all relevant subsystems (compiler, search, history, security, architecture) in a single call. Returns enriched context with diagnostics, history, code context, and security status. Use this FIRST for any question instead of calling individual low-level tools.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Natural-language question (e.g., 'why is the login broken?', 'run a security audit', 'explain the router module')"
                    }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "git_intent_summary".into(),
            description: "LOW-LEVEL — Summarize the intent and direction of recent commits semantically. Groups by category (fix, feature, refactor, security). Prefer `ask_synapseed` for broad project context.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Number of recent commits to analyze (default: 20)",
                        "default": 20
                    }
                }
            }),
        },
        ToolDefinition {
            name: "train_code".into(),
            description: "SPECIALIZED — Evaluate Rust code in an isolated sandbox (The Gym). Compiles, tests, and benchmarks code, returning metrics (compile time, binary size, test results) and a composite score. Use to compare code variants or validate refactoring safety.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "Rust source code to evaluate (injected as lib.rs)"
                    },
                    "tests": {
                        "type": "string",
                        "description": "Optional test code (injected as tests/eval.rs). Use `use eval_project::*;` to import from the source."
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Max evaluation time in seconds (default: 60)",
                        "default": 60
                    },
                    "fuzz": {
                        "type": "boolean",
                        "description": "Enable proptest fuzzing: auto-generate property tests for public functions to discover panics and edge cases",
                        "default": false
                    }
                },
                "required": ["source"]
            }),
        },
        ToolDefinition {
            name: "reset_telemetry".into(),
            description: "LOW-LEVEL — Clear all telemetry data (spans and metrics) from the OTLP receiver. Use to reset the heatmap and start fresh observation.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDefinition {
            name: "janitor_run_now".into(),
            description: "SPECIALIZED — Run the Janitor: scan for clippy warnings and unused dependencies, generate validated fix proposals. Returns findings and actionable proposals.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDefinition {
            name: "janitor_apply_fix".into(),
            description: "SPECIALIZED — Apply a specific Janitor fix proposal by ID. Default: preview only (dry-run). Set `confirm: true` to actually apply. Applied to the actual file, verified with `cargo check`. Automatically reverts if compilation breaks.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "proposal_id": {
                        "type": "string",
                        "description": "The UUID of the proposal to apply (from janitor_run_now results)"
                    },
                    "confirm": {
                        "type": "boolean",
                        "description": "Set to true to actually apply the fix. Default: false (preview only).",
                        "default": false
                    }
                },
                "required": ["proposal_id"]
            }),
        },
        ToolDefinition {
            name: "architect_analyze".into(),
            description: "SPECIALIZED — Analyze project structural health: dependency graph, coupling metrics, cycle detection, god objects, layer violations. Returns architecture score (A-F), violations, and recommendations.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "refresh": {
                        "type": "boolean",
                        "description": "Force a fresh analysis (default: use cached report if available)",
                        "default": false
                    }
                }
            }),
        },
        ToolDefinition {
            name: "semantic_similarity".into(),
            description: "SPECIALIZED — Find code similar to a natural-language query using vector embeddings (cosine similarity). Requires `search.embeddings: true` in DNA config. Use for meaning-based code search beyond keyword matching.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Natural-language query describing the code you're looking for (e.g., 'authentication logic', 'error handling patterns')"
                    },
                    "top_k": {
                        "type": "integer",
                        "description": "Number of results to return (default: 5)",
                        "default": 5
                    },
                    "min_similarity": {
                        "type": "number",
                        "description": "Minimum cosine similarity threshold (default: 0.3)",
                        "default": 0.3
                    }
                },
                "required": ["query"]
            }),
        },
    ]
}

/// Handle a tool call and return the result.
pub fn handle_tool_call(
    name: &str,
    args: &serde_json::Value,
    ctx: &SynapseContext,
) -> ToolCallResult {
    info!(tool = name, "MCP: Tool call");

    match name {
        "get_code_skeleton" => tool_get_code_skeleton(args, ctx),
        "lookup_symbol" => tool_lookup_symbol(args, ctx),
        "scan_security" => tool_scan_security(args, ctx),
        "check_command" => tool_check_command(args, ctx),
        "git_history" => tool_git_history(args, ctx),
        "project_diagnose" => tool_project_diagnose(ctx),
        "consult_architect" => tool_consult_architect(args, ctx),
        "semantic_search" => tool_semantic_search(args, ctx),
        "analyze_history" => tool_analyze_history(args, ctx),
        "get_diagnostics" => tool_get_diagnostics(args, ctx),
        "apply_quick_fix" => tool_apply_quick_fix(args, ctx),
        "ask_synapseed" => tool_ask_synapseed(args, ctx),
        "git_intent_summary" => tool_git_intent_summary(args, ctx),
        "train_code" => tool_train_code(args),
        "reset_telemetry" => tool_reset_telemetry(ctx),
        "janitor_run_now" => tool_janitor_run_now(ctx),
        "janitor_apply_fix" => tool_janitor_apply_fix(args, ctx),
        "architect_analyze" => tool_architect_analyze(args, ctx),
        "semantic_similarity" => tool_semantic_similarity(args, ctx),
        _ => ToolCallResult {
            content: vec![ContentBlock::Text {
                text: format!("Unknown tool: {name}"),
            }],
            is_error: Some(true),
        },
    }
}

fn tool_get_code_skeleton(args: &serde_json::Value, ctx: &SynapseContext) -> ToolCallResult {
    let root = ctx.project_root();
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .map(std::path::PathBuf::from)
        .unwrap_or(root.clone());

    // HCI Req 8: Honest Mirror — warn if path is gitignored
    let gi_warning = check_gitignore_warning(&path, &root).unwrap_or_default();

    // Try shared graph from CortexPlugin for project root
    if path == root {
        if let Some(graph) = ctx.get_extension::<CodeGraph>() {
            let summary = json!({
                "files_indexed": graph.file_count(),
                "symbols_indexed": graph.symbol_count(),
                "path": path.display().to_string(),
            });
            return text_result(format!(
                "{gi_warning}{}",
                serde_json::to_string_pretty(&summary).unwrap_or_default()
            ));
        }
    }

    // Fallback: build ephemeral graph
    let graph = CodeGraph::new();
    if let Err(e) = graph.index_directory(&path) {
        return error_result(format!("Failed to index: {e}"));
    }

    ctx.update_metrics(|m| {
        m.files_indexed = graph.file_count();
        m.symbols_found = graph.symbol_count();
    });

    let summary = json!({
        "files_indexed": graph.file_count(),
        "symbols_indexed": graph.symbol_count(),
        "path": path.display().to_string(),
    });

    text_result(serde_json::to_string_pretty(&summary).unwrap_or_default())
}

fn tool_lookup_symbol(args: &serde_json::Value, ctx: &SynapseContext) -> ToolCallResult {
    let name = match args.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return error_result("Missing required parameter: name".into()),
    };

    // Try shared graph from CortexPlugin
    if let Some(graph) = ctx.get_extension::<CodeGraph>() {
        let results = graph.lookup(name);
        return if results.is_empty() {
            text_result(format!("No symbols found matching '{name}'"))
        } else {
            let json = serde_json::to_string_pretty(&results).unwrap_or_default();
            text_result(format!("Found {} symbol(s):\n{json}", results.len()))
        };
    }

    // Fallback: build ephemeral graph
    let root = ctx.project_root();
    let graph = CodeGraph::new();
    let _ = graph.index_directory(&root);
    let results = graph.lookup(name);

    if results.is_empty() {
        text_result(format!("No symbols found matching '{name}'"))
    } else {
        let json = serde_json::to_string_pretty(&results).unwrap_or_default();
        text_result(format!("Found {} symbol(s):\n{json}", results.len()))
    }
}

fn tool_scan_security(args: &serde_json::Value, ctx: &SynapseContext) -> ToolCallResult {
    let content = match args.get("content").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return error_result("Missing required parameter: content".into()),
    };

    // Try shared guard from HuskPlugin, fallback to defaults
    let default_guard;
    let guard: &SecurityGuard = if let Some(g) = ctx.get_extension::<SecurityGuard>() {
        // Arc<SecurityGuard> -> &SecurityGuard via leak-free reference
        // We need to keep the Arc alive, so we bind it
        // Actually we can just use the Arc directly
        default_guard = g;
        &default_guard
    } else {
        default_guard = std::sync::Arc::new(SecurityGuard::with_defaults());
        &default_guard
    };

    match guard.check(content) {
        Ok(()) => text_result("CLEAN: No sensitive data detected.".into()),
        Err(e) => {
            let sanitized = guard.redact(content);
            text_result(format!("ALERT: {e}\n\nSanitized:\n{sanitized}"))
        }
    }
}

fn tool_check_command(args: &serde_json::Value, ctx: &SynapseContext) -> ToolCallResult {
    let command = match args.get("command").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return error_result("Missing required parameter: command".into()),
    };

    // Try shared sentinel from RootPlugin
    if let Some(sentinel) = ctx.get_extension::<Sentinel>() {
        return match sentinel.evaluate(command) {
            Ok(action) => text_result(format!("ALLOWED ({action:?}): {command}")),
            Err(e) => text_result(format!("DENIED: {e}")),
        };
    }

    // Fallback
    let sentinel = match Sentinel::with_defaults() {
        Ok(s) => s,
        Err(e) => return error_result(format!("Failed to create sentinel: {e}")),
    };

    match sentinel.evaluate(command) {
        Ok(action) => text_result(format!("ALLOWED ({action:?}): {command}")),
        Err(e) => text_result(format!("DENIED: {e}")),
    }
}

/// Get the shared Historian from context, or open a fresh one as fallback.
fn get_historian(
    ctx: &SynapseContext,
) -> std::result::Result<std::sync::Arc<Historian>, ToolCallResult> {
    if let Some(h) = ctx.get_extension::<Historian>() {
        return Ok(h);
    }
    let root = ctx.project_root();
    match Historian::open(&root) {
        Ok(h) => Ok(std::sync::Arc::new(h)),
        Err(e) => Err(error_result(format!("Failed to open git repo: {e}"))),
    }
}

fn tool_git_history(args: &serde_json::Value, ctx: &SynapseContext) -> ToolCallResult {
    let file = match args.get("file").and_then(|v| v.as_str()) {
        Some(f) => f,
        None => return error_result("Missing required parameter: file".into()),
    };
    let start = args.get("start_line").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
    let end = args.get("end_line").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

    let historian = match get_historian(ctx) {
        Ok(h) => h,
        Err(e) => return e,
    };

    match historian.blame_lines(file, start, end) {
        Ok(blame) => {
            if blame.is_empty() {
                text_result(format!("No blame data for {file}:{start}-{end}"))
            } else {
                let json = serde_json::to_string_pretty(&blame).unwrap_or_default();
                text_result(format!("Blame for {file}:{start}-{end}:\n{json}"))
            }
        }
        Err(e) => error_result(format!("Blame failed: {e}")),
    }
}

fn tool_project_diagnose(ctx: &SynapseContext) -> ToolCallResult {
    let root = ctx.project_root();
    let state = ProjectState::detect(&root);

    let mut report = format!("=== SYNAPSEED DIAGNOSTIC ===\n\n{}\n", state.diagnostic());

    // Git info
    if let Ok(historian) = get_historian(ctx) {
        if let Ok(summary) = historian.summary(5) {
            report.push_str(&format!(
                "\n--- Git ---\nBranch: {}\nHEAD: {}\nCommits: {}\nDirty: {}\n",
                summary.branch.as_deref().unwrap_or("detached"),
                &summary.head_commit[..8.min(summary.head_commit.len())],
                summary.total_commits,
                summary.is_dirty,
            ));
            if !summary.recent_commits.is_empty() {
                report.push_str("\nRecent:\n");
                for c in &summary.recent_commits {
                    report.push_str(&format!("  {} | {} | {}\n", c.id, c.author, c.message));
                }
            }
        }
    }

    // Metrics
    let metrics = ctx.metrics();
    report.push_str(&format!(
        "\n--- Metrics ---\nFiles: {} | Symbols: {} | DLP Blocks: {} | Events: {}\n",
        metrics.files_indexed, metrics.symbols_found, metrics.dlp_blocks, metrics.events_broadcast,
    ));

    text_result(report)
}

fn tool_consult_architect(args: &serde_json::Value, ctx: &SynapseContext) -> ToolCallResult {
    let query = match args.get("query").and_then(|v| v.as_str()) {
        Some(q) => q,
        None => return error_result("Missing required parameter: query".into()),
    };

    let dna = ctx.dna();
    let state = ctx.project_state();

    let libs_list = dna
        .preferred_libs
        .iter()
        .map(|(k, v)| format!("  - {k}: {v}"))
        .collect::<Vec<_>>()
        .join("\n");

    let state_summary = match &state {
        ProjectState::HealthyWorkspace {
            build_system,
            file_count,
        } => {
            format!("Healthy ({build_system:?}, {file_count} files)")
        }
        ProjectState::VirginRepo => "Virgin repository (no code yet)".into(),
        ProjectState::PartialSetup { missing, .. } => {
            format!("Partial setup (missing: {})", missing.join(", "))
        }
        ProjectState::Unknown => "Unknown project type".into(),
    };

    let policy = format!(
        "=== ARCHITECTURE POLICY ===\n\n\
         Query: {query}\n\n\
         --- Project DNA ---\n\
         Workspace Strategy: {}\n\
         Naming: core_crate={}, bin_name={}\n\
         DLP Level: {:?}\n\
         Active Plugins: {}\n\n\
         --- Preferred Libraries ---\n\
         {libs_list}\n\n\
         --- Project State ---\n\
         {state_summary}\n\n\
         --- Architecture Guidance ---\n\
         1. Use {} workspace strategy\n\
         2. Async runtime: {}\n\
         3. Error handling: {}\n\
         4. Serialization: {}\n\
         5. Security: DLP level {:?} with fail-closed sentinel\n\
         6. All commands MUST pass through the Sentinel before execution\n\
         7. All outbound content MUST pass through DLP scanning\n",
        dna.workspace_strategy,
        dna.naming.core_crate,
        dna.naming.bin_name,
        dna.dlp_level,
        dna.plugins.join(", "),
        dna.workspace_strategy,
        dna.preferred_libs
            .get("async")
            .map(|s| s.as_str())
            .unwrap_or("tokio"),
        dna.preferred_libs
            .get("error")
            .map(|s| s.as_str())
            .unwrap_or("thiserror"),
        dna.preferred_libs
            .get("json")
            .map(|s| s.as_str())
            .unwrap_or("serde_json"),
        dna.dlp_level,
    );

    text_result(policy)
}

fn tool_semantic_search(args: &serde_json::Value, ctx: &SynapseContext) -> ToolCallResult {
    let query = match args.get("query").and_then(|v| v.as_str()) {
        Some(q) => q,
        None => return error_result("Missing required parameter: query".into()),
    };
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

    // Try to use the persistent index from SearchPlugin
    let results = if let Some(index) = ctx.get_extension::<SemanticIndex>() {
        index.search(query, limit)
    } else {
        // Fallback: build an ephemeral index on demand
        info!("Search: No persistent index, building ephemeral index");
        let root = ctx.project_root();
        let graph = CodeGraph::new();
        if let Err(e) = graph.index_directory(&root) {
            return error_result(format!("Failed to index project: {e}"));
        }
        let index = match SemanticIndex::new() {
            Ok(idx) => idx,
            Err(e) => return error_result(format!("Failed to create search index: {e}")),
        };
        let files = graph.all_files();
        index.index_all(&files, &root);
        index.search(query, limit)
    };

    if results.is_empty() {
        text_result(format!("No results found for: \"{query}\""))
    } else {
        let json = serde_json::to_string_pretty(&results).unwrap_or_default();
        text_result(format!(
            "Found {} result(s) for \"{query}\":\n{json}",
            results.len()
        ))
    }
}

fn tool_analyze_history(args: &serde_json::Value, ctx: &SynapseContext) -> ToolCallResult {
    let file = match args.get("file").and_then(|v| v.as_str()) {
        Some(f) => f,
        None => return error_result("Missing required parameter: file".into()),
    };
    let start_line = args
        .get("start_line")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);
    let end_line = args
        .get("end_line")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    let historian = match get_historian(ctx) {
        Ok(h) => h,
        Err(e) => return e,
    };

    match historian.analyze_history(file, start_line, end_line) {
        Ok(analysis) => {
            let json = serde_json::to_string_pretty(&analysis).unwrap_or_default();
            let range_str = match analysis.line_range {
                Some((s, e)) => format!(":{s}-{e}"),
                None => String::new(),
            };
            text_result(format!(
                "=== History Analysis: {file}{range_str} ===\n\
                 Commits: {} | Hotspot: {:.1} | Risk: {}\n\n{json}",
                analysis.total_commits,
                analysis.hotspot_score,
                analysis.semantic_summary.risk_indicator,
            ))
        }
        Err(e) => error_result(format!("History analysis failed: {e}")),
    }
}

fn tool_get_diagnostics(args: &serde_json::Value, ctx: &SynapseContext) -> ToolCallResult {
    use synapseed_shadow_check::runner::MinSeverity;

    let store = match ctx.get_extension::<DiagnosticStore>() {
        Some(s) => s,
        None => return text_result("Shadow compiler not active (no Cargo.toml found or not initialized). Run `synapseed init` first.".into()),
    };

    let file_filter = args.get("file").and_then(|v| v.as_str());
    let min_severity = args
        .get("min_severity")
        .and_then(|v| v.as_str())
        .map(MinSeverity::from_str_loose)
        .unwrap_or(MinSeverity::Warning);

    let snap = store.filtered_snapshot(min_severity);
    let diagnostics = match file_filter {
        Some(file) => snap
            .diagnostics
            .iter()
            .filter(|d| d.file_path == file || file.ends_with(&d.file_path))
            .cloned()
            .collect(),
        None => snap.diagnostics,
    };

    if diagnostics.is_empty() {
        text_result(format!(
            "CLEAN: No diagnostics at severity {:?}+. Last check took {}ms.",
            min_severity, snap.last_check_ms
        ))
    } else {
        let json = serde_json::to_string_pretty(&diagnostics).unwrap_or_default();
        text_result(format!(
            "{} error(s), {} warning(s) (check took {}ms, filter: {:?}+):\n{json}",
            snap.error_count, snap.warning_count, snap.last_check_ms, min_severity
        ))
    }
}

fn tool_apply_quick_fix(args: &serde_json::Value, ctx: &SynapseContext) -> ToolCallResult {
    let file = match args.get("file").and_then(|v| v.as_str()) {
        Some(f) => f,
        None => return error_result("Missing required parameter: file".into()),
    };
    let error_code = match args.get("error_code").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return error_result("Missing required parameter: error_code".into()),
    };

    let store = match ctx.get_extension::<DiagnosticStore>() {
        Some(s) => s,
        None => return error_result("Shadow compiler not active.".into()),
    };

    match store.apply_fix(file, error_code) {
        Ok(msg) => text_result(msg),
        Err(e) => error_result(e),
    }
}

fn tool_ask_synapseed(args: &serde_json::Value, ctx: &SynapseContext) -> ToolCallResult {
    let query = match args.get("query").and_then(|v| v.as_str()) {
        Some(q) => q,
        None => return error_result("Missing required parameter: query".into()),
    };

    let result = synapseed_whisper::router::ask(query, ctx);

    let json = serde_json::to_string_pretty(&result).unwrap_or_default();

    // Smart Context Injection: the LLM prompt is enriched with
    // the smart_context summary followed by the full JSON data.
    text_result(format!(
        "{}\n\n--- Full Context ---\n{json}",
        result.smart_context
    ))
}

fn tool_git_intent_summary(args: &serde_json::Value, ctx: &SynapseContext) -> ToolCallResult {
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

    let historian = match get_historian(ctx) {
        Ok(h) => h,
        Err(e) => return e,
    };

    match historian.summarize_intent(limit) {
        Ok(intent) => {
            let json = serde_json::to_string_pretty(&intent).unwrap_or_default();
            text_result(format!("{}\n\n{json}", intent.summary))
        }
        Err(e) => error_result(format!("Intent summary failed: {e}")),
    }
}

fn tool_train_code(args: &serde_json::Value) -> ToolCallResult {
    let source = match args.get("source").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return error_result("Missing required parameter: source".into()),
    };
    let tests = args
        .get("tests")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let timeout = args
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(60);

    let fuzz = args
        .get("fuzz")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut scenario = Scenario::new(source).with_timeout(timeout).with_fuzz(fuzz);
    if !tests.is_empty() {
        scenario = scenario.with_tests(tests);
    }

    let trainer = Trainer::new();
    match trainer.evaluate(&scenario) {
        Ok(report) => {
            let score = report.score();
            let json = serde_json::to_string_pretty(&report).unwrap_or_default();

            let fuzz_summary = report.fuzz.as_ref().map_or(String::new(), |f| {
                if f.failures.is_empty() {
                    format!(" | Fuzz: {}/{} passed", f.fuzzed_functions, f.fuzzed_functions)
                } else {
                    format!(" | Fuzz: {} failures in {} functions", f.failures.len(), f.fuzzed_functions)
                }
            });

            text_result(format!(
                "=== GYM REPORT ===\nScore: {score:.2}/1.00 | Success: {} | Compiled: {} | Warnings: {} | Errors: {}{}{}\n\nCompile: {}ms | Binary: {} bytes | Tests: {}ms\n\n{json}",
                report.success,
                report.compilation.compiled,
                report.compilation.warnings,
                report.compilation.errors,
                report.tests.as_ref().map_or(String::new(), |t| format!(" | Tests: {}/{} passed", t.passed, t.total)),
                fuzz_summary,
                report.metrics.compile_time_ms,
                report.metrics.binary_size_bytes,
                report.metrics.test_time_ms,
            ))
        }
        Err(e) => error_result(format!("Gym evaluation failed: {e}")),
    }
}

fn tool_reset_telemetry(ctx: &SynapseContext) -> ToolCallResult {
    match ctx.get_extension::<SpanStore>() {
        Some(store) => {
            let stats = store.stats();
            store.reset();
            text_result(format!(
                "Telemetry reset. Cleared {} spans across {} locations.",
                stats.total_spans, stats.unique_locations
            ))
        }
        None => text_result("Telemetry sink not active.".into()),
    }
}

fn tool_janitor_run_now(ctx: &SynapseContext) -> ToolCallResult {
    let store = match ctx.get_extension::<ProposalStore>() {
        Some(s) => s,
        None => return error_result("Janitor plugin not active.".into()),
    };

    // Prevent double-scan (atomic compare-exchange)
    if !store.start_scanning() {
        return text_result(
            "Janitor scan already in progress. Check `synapseed://janitor/proposals` for results."
                .into(),
        );
    }

    // If there's a previous scan result, include it as context
    let previous = store.last_scan().map(|s| {
        format!(
            " (previous scan: {} issues, {} proposals at {})",
            s.clippy_issues, s.proposals_created, s.completed_at
        )
    });

    let root = ctx.project_root().to_path_buf();
    let bg_store = store.clone();

    // Run scan in background thread — return immediately
    std::thread::spawn(move || {
        let janitor = Janitor::new(bg_store.clone());
        match janitor.scan(&root) {
            Ok(result) => {
                bg_store.finish_scan(synapseed_janitor::LastScan {
                    completed_at: chrono::Utc::now().to_rfc3339(),
                    clippy_issues: result.clippy_issues,
                    fixable_issues: result.fixable_issues,
                    unused_deps: result.unused_deps.len(),
                    proposals_created: result.proposals_created,
                    error: None,
                });
            }
            Err(e) => {
                bg_store.finish_scan(synapseed_janitor::LastScan {
                    completed_at: chrono::Utc::now().to_rfc3339(),
                    clippy_issues: 0,
                    fixable_issues: 0,
                    unused_deps: 0,
                    proposals_created: 0,
                    error: Some(e.to_string()),
                });
            }
        }
    });

    text_result(format!(
        "Janitor scan started in background.{}\n\nResults will appear in `synapseed://janitor/proposals`. You can also call `janitor_run_now` again — it will show the results once the scan completes.",
        previous.unwrap_or_default()
    ))
}

fn tool_janitor_apply_fix(args: &serde_json::Value, ctx: &SynapseContext) -> ToolCallResult {
    let proposal_id = match args.get("proposal_id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return error_result("Missing required parameter: proposal_id".into()),
    };
    let confirm = args
        .get("confirm")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let store = match ctx.get_extension::<ProposalStore>() {
        Some(s) => s,
        None => return error_result("Janitor plugin not active.".into()),
    };

    // HCI Req 3 (Safety Net): dry-run by default — preview what WOULD change
    if !confirm {
        return match store.get(proposal_id) {
            Some(proposal) => {
                text_result(format!(
                    "PREVIEW (dry-run): Would apply fix to {}:{}\n\
                     - Description: {}\n\
                     - Original:\n{}\n\
                     - Fixed:\n{}\n\n\
                     Call again with `confirm: true` to apply this fix.",
                    proposal.file_path,
                    proposal.line_start,
                    proposal.description,
                    proposal.original_code,
                    proposal.fixed_code,
                ))
            }
            None => error_result(format!("No proposal found with ID: {proposal_id}")),
        };
    }

    let janitor = Janitor::new(store);
    let root = ctx.project_root();

    match janitor.apply(proposal_id, &root) {
        Ok(msg) => text_result(format!("Fix applied successfully.\n{msg}")),
        Err(e) => error_result(format!("Failed to apply fix: {e}")),
    }
}

fn tool_architect_analyze(args: &serde_json::Value, ctx: &SynapseContext) -> ToolCallResult {
    let refresh = args
        .get("refresh")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Try cached report from ArchitectPlugin
    if !refresh {
        if let Some(store) = ctx.get_extension::<ReportStore>() {
            if let Some(report) = store.get() {
                let json = serde_json::to_string_pretty(&report).unwrap_or_default();
                return text_result(format!(
                    "=== ARCHITECTURE REPORT ===\nScore: {}/100 (Grade: {})\nModules: {} | Edges: {} | Violations: {}\n\n{json}",
                    report.score, report.grade, report.module_count, report.edge_count, report.violations.len()
                ));
            }
        }
    }

    // Build fresh report (or no cached one exists)
    let graph = match ctx.get_extension::<CodeGraph>() {
        Some(g) => g,
        None => {
            // Fallback: build ephemeral graph
            let root = ctx.project_root();
            let g = CodeGraph::new();
            if let Err(e) = g.index_directory(&root) {
                return error_result(format!("Failed to index project: {e}"));
            }
            std::sync::Arc::new(g)
        }
    };

    let dna = ctx.dna();
    let mut dep_graph = synapseed_architect::DependencyGraph::build(&graph);
    dep_graph.compute_metrics();

    let config = synapseed_architect::linter::LinterConfig::from_dna(&dna.architect);
    let violations = synapseed_architect::linter::lint(&dep_graph, &config);
    let report = synapseed_architect::blueprint::generate_report(&dep_graph, violations);

    // Cache the report if store exists
    if let Some(store) = ctx.get_extension::<ReportStore>() {
        store.set(report.clone());
    }

    let json = serde_json::to_string_pretty(&report).unwrap_or_default();
    text_result(format!(
        "=== ARCHITECTURE REPORT ===\nScore: {}/100 (Grade: {})\nModules: {} | Edges: {} | Violations: {}\n\n{json}",
        report.score, report.grade, report.module_count, report.edge_count, report.violations.len()
    ))
}

fn tool_semantic_similarity(args: &serde_json::Value, ctx: &SynapseContext) -> ToolCallResult {
    let query = match args.get("query").and_then(|v| v.as_str()) {
        Some(q) => q.trim(),
        None => return error_result("Missing required parameter: query".into()),
    };
    if query.is_empty() {
        return error_result("Query must not be empty".into());
    }
    let top_k = args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
    let min_similarity = args
        .get("min_similarity")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.3) as f32;

    #[cfg(feature = "embeddings")]
    {
        use synapseed_search::embeddings::EmbeddingEngine;
        use synapseed_search::vector_index::VectorIndex;

        let engine = match ctx.get_extension::<EmbeddingEngine>() {
            Some(e) => e,
            None => {
                return text_result(
                    "Embeddings not available. Enable with `search.embeddings: true` in your DNA config (.synapseed/dna.yaml).".into(),
                );
            }
        };

        let vector_index = match ctx.get_extension::<VectorIndex>() {
            Some(vi) => vi,
            None => {
                return text_result("Vector index not ready. Embeddings may still be loading.".into());
            }
        };

        let query_vector = match engine.embed(query) {
            Ok(v) => v,
            Err(e) => return error_result(format!("Failed to embed query: {e}")),
        };

        let results = vector_index.search(&query_vector, top_k);
        let filtered: Vec<_> = results
            .into_iter()
            .filter(|r| r.similarity >= min_similarity)
            .collect();

        if filtered.is_empty() {
            text_result(format!(
                "No results above similarity threshold ({min_similarity}) for: \"{query}\""
            ))
        } else {
            let json = serde_json::to_string_pretty(&filtered).unwrap_or_default();
            text_result(format!(
                "Found {} similar symbol(s) for \"{query}\":\n{json}",
                filtered.len()
            ))
        }
    }

    #[cfg(not(feature = "embeddings"))]
    {
        let _ = (query, top_k, min_similarity, ctx);
        text_result(
            "Embeddings not compiled. Rebuild with the `embeddings` feature enabled.".into(),
        )
    }
}

/// Check if a file path is listed in .gitignore (HCI Req 8: Honest Mirror).
/// Returns a warning string if the file IS ignored, or None if tracked.
fn check_gitignore_warning(path: &Path, root: &Path) -> Option<String> {
    let gi_path = root.join(".gitignore");
    if !gi_path.exists() {
        return None;
    }
    let (gi, _) = ignore::gitignore::Gitignore::new(&gi_path);
    if gi.matched(path, path.is_dir()).is_ignore() {
        Some(format!(
            "WARNING: {} is listed in .gitignore. Results may not reflect tracked code.\n\n",
            path.display()
        ))
    } else {
        None
    }
}

fn text_result(text: String) -> ToolCallResult {
    ToolCallResult {
        content: vec![ContentBlock::Text { text }],
        is_error: None,
    }
}

fn error_result(message: String) -> ToolCallResult {
    ToolCallResult {
        content: vec![ContentBlock::Text { text: message }],
        is_error: Some(true),
    }
}
