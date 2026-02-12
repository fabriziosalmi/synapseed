//! MCP Tool definitions and handlers.
//!
//! Each tool maps to an internal SYNAPSEED capability.
//! Tool implementations live in sub-modules; this file owns the schema
//! registry, the dispatch table, and shared helpers.

mod architect;
mod diagnose;
mod diagnostics;
mod history;
mod janitor;
mod oracle;
mod search;
mod security;
mod skeleton;
mod symbol;
mod synapseed;
mod telemetry;
mod train;

use std::path::Path;

use serde_json::json;
use tracing::info;

use synapseed_chronos::historian::Historian;
use synapseed_core::context::SynapseContext;

use crate::protocol::{ContentBlock, ToolCallResult, ToolDefinition};

// ── Schema registry ─────────────────────────────────────────────────

/// Return all available tool definitions.
pub fn list_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "hoist".into(),
            description: "LOW-LEVEL — Index a project directory and return its AST skeleton (files, symbols, structure). Prefer `ask` for holistic queries; use this only when you need raw symbol data.".into(),
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
            name: "lookup".into(),
            description: "LOW-LEVEL — Find a symbol by name across the entire project. Returns file path, line numbers, and signature. Prefer `ask` unless you know the exact symbol name.".into(),
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
            name: "scan".into(),
            description: "LOW-LEVEL — Scan text content for sensitive data (API keys, passwords, tokens) AND code security anti-patterns (SQL injection, XSS, command injection, path traversal). Returns findings or CLEAN status. Use `mode` to select scan type.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "Text content to scan"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["all", "dlp", "patterns"],
                        "description": "Scan mode: 'all' (default) = DLP + code patterns, 'dlp' = secrets only, 'patterns' = code anti-patterns only"
                    }
                },
                "required": ["content"]
            }),
        },
        ToolDefinition {
            name: "check".into(),
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
            name: "blame".into(),
            description: "LOW-LEVEL — Get git blame/history for a file. Shows who changed what and why. Prefer `analyze` for richer insights or `ask` for holistic context.".into(),
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
            name: "diagnose".into(),
            description: "LOW-LEVEL — Run a full diagnostic on the project: detect state (virgin/partial/healthy), build system, git status, active plugins. Included automatically in `ask` responses.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDefinition {
            name: "consult".into(),
            description: "LOW-LEVEL — Consult the project's architecture policy (DNA config). Returns preferred libraries, workspace strategy, naming conventions. Use `architect` for structural health or `ask` for holistic answers.".into(),
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
            name: "search".into(),
            description: "LOW-LEVEL — Search for code by concept (Tantivy keyword index). Finds symbols by name, signature, doc comments. Supports fuzzy matching. Prefer `ask` for broad queries; use this for targeted symbol search.".into(),
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
            name: "diagnostics".into(),
            description: "LOW-LEVEL — Get current compiler diagnostics from the background shadow compiler. Optionally filter by file path and/or severity. Included automatically in `ask` responses when relevant.".into(),
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
            name: "analyze".into(),
            description: "LOW-LEVEL — Analyze file history: churn/hotspot score, co-change patterns, semantic commit classification, risk assessment. Use for deep dives into a specific file; `ask` includes this automatically when relevant.".into(),
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
            name: "quickfix".into(),
            description: "LOW-LEVEL — Apply a compiler-suggested fix automatically. Only applies 'MachineApplicable' suggestions from rustc. Call `diagnostics` first to find the error code.".into(),
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
            name: "ask".into(),
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
            name: "intent".into(),
            description: "LOW-LEVEL — Summarize the intent and direction of recent commits semantically. Groups by category (fix, feature, refactor, security). Prefer `ask` for broad project context.".into(),
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
            name: "train".into(),
            description: "SPECIALIZED — Evaluate Rust code in an isolated sandbox (The Gym). Compiles, tests, benchmarks, and optionally runs adversarial mutation testing, returning metrics (compile time, binary size, test results, mutation score) and a composite score. Use to compare code variants or validate refactoring safety.".into(),
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
                        "description": "Enable proptest fuzzing: auto-generate property tests for public functions",
                        "default": false
                    },
                    "adversarial": {
                        "type": "boolean",
                        "description": "Enable adversarial mutation testing: apply controlled mutations to measure test suite effectiveness (mutation score)",
                        "default": false
                    }
                },
                "required": ["source"]
            }),
        },
        ToolDefinition {
            name: "reset-telemetry".into(),
            description: "LOW-LEVEL — Clear all telemetry data (spans and metrics) from the OTLP receiver. Use to reset the heatmap and start fresh observation.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDefinition {
            name: "janitor".into(),
            description: "SPECIALIZED — Run the Janitor: scan for clippy warnings and unused dependencies, generate validated fix proposals. Returns findings and actionable proposals.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDefinition {
            name: "janitor-fix".into(),
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
            name: "architect".into(),
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
            name: "oracle".into(),
            description: "SPECIALIZED — Auto-repair drifted documentation. Updates version numbers, crate counts, and MCP tool/resource counts in README.md to match the actual codebase. Returns a list of changes made.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDefinition {
            name: "similar".into(),
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

// ── Dispatch table ──────────────────────────────────────────────────

/// Canonical tool names (short, CLI-aligned).
const TOOL_NAMES: &[&str] = &[
    "hoist", "lookup", "scan", "check", "blame", "diagnose", "consult",
    "search", "diagnostics", "analyze", "quickfix", "ask", "intent",
    "train", "reset-telemetry", "janitor", "janitor-fix", "architect",
    "oracle", "similar",
];

/// Resolve a tool name: canonical names pass through, legacy names are mapped.
fn resolve_tool_name(name: &str) -> Option<&'static str> {
    match name {
        // ── Canonical (short) names ─────────────────────────────
        "hoist" => Some("hoist"),
        "lookup" => Some("lookup"),
        "scan" => Some("scan"),
        "check" => Some("check"),
        "blame" => Some("blame"),
        "diagnose" => Some("diagnose"),
        "consult" => Some("consult"),
        "search" => Some("search"),
        "diagnostics" => Some("diagnostics"),
        "analyze" => Some("analyze"),
        "quickfix" => Some("quickfix"),
        "ask" => Some("ask"),
        "intent" => Some("intent"),
        "train" => Some("train"),
        "reset-telemetry" => Some("reset-telemetry"),
        "janitor" => Some("janitor"),
        "janitor-fix" => Some("janitor-fix"),
        "architect" => Some("architect"),
        "oracle" => Some("oracle"),
        "similar" => Some("similar"),
        // ── Legacy aliases (backward-compat) ────────────────────
        "get_code_skeleton" => Some("hoist"),
        "lookup_symbol" => Some("lookup"),
        "scan_security" => Some("scan"),
        "check_command" => Some("check"),
        "git_history" => Some("blame"),
        "project_diagnose" => Some("diagnose"),
        "consult_architect" => Some("consult"),
        "semantic_search" => Some("search"),
        "get_diagnostics" => Some("diagnostics"),
        "analyze_history" => Some("analyze"),
        "apply_quick_fix" => Some("quickfix"),
        "ask_synapseed" | "whisper" => Some("ask"),
        "git_intent_summary" => Some("intent"),
        "train_code" => Some("train"),
        "reset_telemetry" => Some("reset-telemetry"),
        "janitor_run_now" => Some("janitor"),
        "janitor_apply_fix" => Some("janitor-fix"),
        "architect_analyze" => Some("architect"),
        "oracle_fix_docs" => Some("oracle"),
        "semantic_similarity" => Some("similar"),
        _ => None,
    }
}

/// Execute the handler for a resolved canonical tool name.
fn dispatch_tool(
    canonical: &str,
    args: &serde_json::Value,
    ctx: &SynapseContext,
) -> ToolCallResult {
    match canonical {
        "hoist" => skeleton::tool_get_code_skeleton(args, ctx),
        "lookup" => symbol::tool_lookup_symbol(args, ctx),
        "scan" => security::tool_scan_security(args, ctx),
        "check" => security::tool_check_command(args, ctx),
        "blame" => history::tool_git_history(args, ctx),
        "diagnose" => diagnose::tool_project_diagnose(ctx),
        "consult" => diagnose::tool_consult_architect(args, ctx),
        "search" => search::tool_semantic_search(args, ctx),
        "diagnostics" => diagnostics::tool_get_diagnostics(args, ctx),
        "analyze" => history::tool_analyze_history(args, ctx),
        "quickfix" => diagnostics::tool_apply_quick_fix(args, ctx),
        "ask" => synapseed::tool_ask_synapseed(args, ctx),
        "intent" => history::tool_git_intent_summary(args, ctx),
        "train" => train::tool_train_code(args),
        "reset-telemetry" => telemetry::tool_reset_telemetry(ctx),
        "janitor" => janitor::tool_janitor_run_now(ctx),
        "janitor-fix" => janitor::tool_janitor_apply_fix(args, ctx),
        "architect" => architect::tool_architect_analyze(args, ctx),
        "oracle" => oracle::tool_oracle_fix_docs(ctx),
        "similar" => search::tool_semantic_similarity(args, ctx),
        _ => error_result(format!("Internal dispatch error: unknown canonical tool '{canonical}'")),
    }
}

/// Levenshtein edit distance (inline, no external deps).
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    let mut prev = (0..=n).collect::<Vec<_>>();
    let mut curr = vec![0; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

/// Handle a tool call and return the result.
/// Supports canonical names, legacy aliases, and fuzzy matching.
pub fn handle_tool_call(
    name: &str,
    args: &serde_json::Value,
    ctx: &SynapseContext,
) -> ToolCallResult {
    info!(tool = name, "MCP: Tool call");

    // 1. Exact match (canonical or legacy alias)
    if let Some(canonical) = resolve_tool_name(name) {
        if canonical != name {
            info!(alias = name, resolved = canonical, "MCP: Resolved legacy alias");
        }
        return dispatch_tool(canonical, args, ctx);
    }

    // 2. Fuzzy match — find closest canonical name
    let mut best: Option<(&str, usize)> = None;
    for &tool in TOOL_NAMES {
        let dist = levenshtein(name, tool);
        if best.map_or(true, |(_, d)| dist < d) {
            best = Some((tool, dist));
        }
    }

    if let Some((suggestion, dist)) = best {
        if dist <= 3 {
            info!(
                typo = name,
                resolved = suggestion,
                distance = dist,
                "MCP: Fuzzy-resolved tool name"
            );
            let mut result = dispatch_tool(suggestion, args, ctx);
            // Prepend "Did you mean..." to the output
            let prefix = format!(
                "Did you mean '{suggestion}'? (resolved from '{name}', edit distance: {dist})\n\n"
            );
            if let Some(ContentBlock::Text { text }) = result.content.first_mut() {
                *text = format!("{prefix}{text}");
            }
            return result;
        }
        // 3. Semantic catch-all — if the name looks like natural language,
        // redirect to `ask` instead of erroring. This handles small models
        // that write a question as the tool name instead of calling `ask`.
        if name.contains(' ') || name.contains('?') || name.len() > 20 {
            info!(
                input = name,
                "MCP: Natural-language tool name detected, redirecting to ask"
            );
            let query = if let Some(q) = args.get("query").and_then(|v| v.as_str()) {
                q.to_string()
            } else {
                name.to_string()
            };
            let mut result = dispatch_tool("ask", &json!({"query": query}), ctx);
            let prefix = format!(
                "[Redirected to ask] Input '{name}' is not a tool name.\n\n"
            );
            if let Some(ContentBlock::Text { text }) = result.content.first_mut() {
                *text = format!("{prefix}{text}");
            }
            return result;
        }

        // Distance too large — suggest but don't auto-execute
        return error_result(format!(
            "Unknown tool: '{name}'. Did you mean '{suggestion}'?\n\nAvailable tools: {}",
            TOOL_NAMES.join(", ")
        ));
    }

    error_result(format!(
        "Unknown tool: '{name}'. Available tools: {}",
        TOOL_NAMES.join(", ")
    ))
}

// ── Shared helpers ──────────────────────────────────────────────────

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
