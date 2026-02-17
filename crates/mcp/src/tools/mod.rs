//! MCP Tool definitions and handlers.
//!
//! Each tool maps to an internal SYNAPSEED capability.
//! Tool implementations live in sub-modules; this file owns the schema
//! registry, the dispatch table, and shared helpers.
//!
//! ## Tool Routing Hierarchy
//!
//! Descriptions are engineered for LLM tool selection:
//! - **PRIMARY** (`ask`): Single entry point, orchestrates everything
//! - **CORE**: Direct operations for targeted use
//! - **SPECIALIZED**: Expert-level analysis and mutation
//! - **LOW-LEVEL**: Granular access to subsystems

mod approve_fix;
mod architect;
#[cfg(feature = "bench")]
mod bench;
#[cfg(feature = "decompiler")]
mod decompile;
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
mod verify;

use std::path::Path;

use serde_json::json;
use tracing::{error, info};

use synapseed_chronos::historian::Historian;
use synapseed_core::context::SynapseContext;

use crate::protocol::{ContentBlock, ToolAnnotations, ToolCallResult, ToolDefinition};

/// Helper: annotations for a read-only, idempotent tool (most analysis tools).
fn ro() -> Option<ToolAnnotations> {
    Some(ToolAnnotations {
        title: None,
        read_only_hint: Some(true),
        destructive_hint: Some(false),
        idempotent_hint: Some(true),
        open_world_hint: Some(false),
    })
}

/// Helper: annotations for a mutating tool.
fn rw(destructive: bool) -> Option<ToolAnnotations> {
    Some(ToolAnnotations {
        title: None,
        read_only_hint: Some(false),
        destructive_hint: Some(destructive),
        idempotent_hint: Some(false),
        open_world_hint: Some(false),
    })
}

// ── Schema registry ─────────────────────────────────────────────────

/// Return all 24 MCP tool definitions.
///
/// Tools: ask (primary entry point for natural-language queries), search, lookup,
/// scan, check, hoist, blame, analyze, diagnostics, quickfix, diagnose, consult,
/// intent, verify_path, similar, train, janitor, janitor-fix, architect, oracle,
/// reset-telemetry.
///
/// `ask` orchestrates all subsystems in a single call. Other tools provide
/// targeted access to individual capabilities.
pub fn list_tools() -> Vec<ToolDefinition> {
    vec![
        // ════════════════════════════════════════════════════════
        // PRIMARY — The single entry point. Route ALL questions here first.
        // ════════════════════════════════════════════════════════
        ToolDefinition {
            name: "ask".into(),
            description: "PRIMARY — The intelligent entry point for ANY code question. ALWAYS call this tool FIRST. \
                It automatically orchestrates semantic search, compiler diagnostics, git history, security scanning, \
                and architecture analysis in a SINGLE call — no need to chain individual tools. Returns a comprehensive, \
                model-tier-adapted context with ranked code symbols, active errors, recent changes, and security status. \
                Handles natural language: 'why is login broken?', 'explain the router', 'run a security audit'. \
                Set raw=true to inject exact source code for small models that need real code, not summaries.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Natural-language question about the codebase (e.g., 'why is the login broken?', 'explain the router module', 'run a security audit')"
                    },
                    "raw": {
                        "type": "boolean",
                        "description": "When true, inject the EXACT source code of discovered symbols into the response (Direct Symbol Injection). Set this for small/local models that need verbatim code.",
                        "default": false
                    }
                },
                "required": ["query"]
            }),
            annotations: ro(),
        },

        // ════════════════════════════════════════════════════════
        // CORE — Targeted operations for specific needs.
        // Use these when you know exactly what you need.
        // ════════════════════════════════════════════════════════
        ToolDefinition {
            name: "search".into(),
            description: "CORE — Semantic code search powered by Tantivy with BM25→Prefix→Fuzzy cascade. \
                Finds symbols by name, signature, or documentation concept. Use this for TARGETED symbol \
                lookup when you know what you're looking for. For broad questions, use `ask` instead — \
                it includes search results automatically.".into(),
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
            annotations: ro(),
        },
        ToolDefinition {
            name: "lookup".into(),
            description: "CORE — Resolve a symbol by exact name across the entire project. Returns file path, line range, kind, \
                and full signature. Use when you KNOW the exact function/struct/trait name. For fuzzy or concept \
                search, use `search`. For broad questions, use `ask`.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Exact symbol name to find (function, struct, trait, variable)"
                    }
                },
                "required": ["name"]
            }),
            annotations: ro(),
        },
        ToolDefinition {
            name: "scan".into(),
            description: "CORE — Security scanner: detects API keys, passwords, tokens, PII, AND code vulnerability \
                patterns (SQL injection, XSS, command injection, path traversal). ALWAYS scan code before sharing \
                anything containing configuration, credentials, or user input handling. Returns CLEAN or ALERT \
                with findings.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "Text content to scan for secrets and vulnerability patterns"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["all", "dlp", "patterns"],
                        "description": "Scan mode: 'all' (default) = DLP + code patterns, 'dlp' = secrets only, 'patterns' = code vulnerabilities only"
                    }
                },
                "required": ["content"]
            }),
            annotations: ro(),
        },
        ToolDefinition {
            name: "check".into(),
            description: "CORE — Command safety validator. Evaluates any shell command against the security policy \
                BEFORE execution. Returns ALLOWED or DENIED with reason. ALWAYS call this before running or \
                suggesting shell commands to prevent destructive operations (rm -rf, curl|sh, chmod 777, etc.).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Shell command to evaluate against the security policy"
                    }
                },
                "required": ["command"]
            }),
            annotations: ro(),
        },

        // ════════════════════════════════════════════════════════
        // DEEP-DIVE — Analysis and diagnostics
        // ════════════════════════════════════════════════════════
        ToolDefinition {
            name: "hoist".into(),
            description: "Index a project directory and return its complete AST skeleton (files, symbols, relationships). \
                Use for architecture overview or when you need the full symbol graph. Note: `ask` automatically \
                includes relevant symbols — call `hoist` only when you need the RAW structure dump.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to directory to index (default: project root)"
                    }
                }
            }),
            annotations: ro(),
        },
        ToolDefinition {
            name: "blame".into(),
            description: "Git blame and history for a specific file region. Shows who changed what, when, and why. \
                Use for understanding code evolution or investigating when a bug was introduced. \
                For richer analysis with churn metrics and risk scoring, use `analyze` instead.".into(),
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
            annotations: ro(),
        },
        ToolDefinition {
            name: "analyze".into(),
            description: "Deep file history analysis: churn/hotspot score, co-change patterns, semantic commit classification, \
                and risk assessment. Use for understanding WHY code is complex or fragile. Returns quantified risk \
                metrics and change velocity data.".into(),
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
            annotations: ro(),
        },
        ToolDefinition {
            name: "diagnostics".into(),
            description: "Live compiler diagnostics from the background shadow compiler. Returns current errors, warnings, \
                and available quick-fixes with file locations and severity. Filter by file or minimum severity. \
                Note: `ask` includes diagnostics automatically when they're relevant.".into(),
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
            annotations: ro(),
        },
        ToolDefinition {
            name: "quickfix".into(),
            description: "Auto-apply a compiler-suggested fix. Only applies safe MachineApplicable suggestions from rustc. \
                Call `diagnostics` first to identify the error code, then call this to fix it automatically.".into(),
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
            annotations: rw(false),
        },
        ToolDefinition {
            name: "diagnose".into(),
            description: "Full project diagnostic: detects project state (virgin/partial/healthy), build system, git status, \
                and active plugin health. Use to understand the runtime situation of the project.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
            annotations: ro(),
        },
        ToolDefinition {
            name: "consult".into(),
            description: "Query the project's architecture policy (DNA config). Returns preferred libraries, workspace strategy, \
                naming conventions, and team decisions. Use before making structural decisions to stay aligned \
                with project conventions.".into(),
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
            annotations: ro(),
        },
        ToolDefinition {
            name: "intent".into(),
            description: "Semantic summary of recent commit intent. Groups commits by category (fix, feature, refactor, security) \
                and extracts scope hints. Use to understand what the team has been working on recently.".into(),
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
            annotations: ro(),
        },
        ToolDefinition {
            name: "verify_path".into(),
            description: "Verify a file path exists in the project. Returns existence, size, and detected language. \
                Use this BEFORE citing file paths to prevent hallucination of non-existent files.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to project root to verify"
                    }
                },
                "required": ["path"]
            }),
            annotations: ro(),
        },
        ToolDefinition {
            name: "similar".into(),
            description: "SPECIALIZED — Vector embedding similarity search (cosine). \
                Finds code semantically related to a natural-language description, even when keywords don't match. \
                Requires `search.embeddings: true` in DNA config. Use for meaning-based code discovery beyond BM25.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Natural-language description of code you're looking for (e.g., 'authentication logic', 'error handling patterns')"
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
            annotations: ro(),
        },

        // ════════════════════════════════════════════════════════
        // SPECIALIZED — Expert operations (mutation, evaluation)
        // ════════════════════════════════════════════════════════
        ToolDefinition {
            name: "train".into(),
            description: "SPECIALIZED — Evaluate Rust code in an isolated sandbox (The Gym). Compiles, tests, benchmarks, \
                and optionally runs adversarial mutation testing. Returns metrics (compile time, binary size, test results, \
                mutation score) and a composite quality score. Use to validate code before committing.".into(),
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
            annotations: Some(ToolAnnotations {
                title: Some("Code Gym".into()),
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                idempotent_hint: Some(true),
                open_world_hint: Some(false),
            }),
        },
        ToolDefinition {
            name: "janitor".into(),
            description: "SPECIALIZED — Scan for clippy warnings and unused dependencies. Generates validated fix proposals \
                with UUIDs. Proposals are safe to preview — use `janitor-fix` to apply them.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
            annotations: ro(),
        },
        ToolDefinition {
            name: "janitor-fix".into(),
            description: "SPECIALIZED — Apply a Janitor fix proposal by UUID. Preview-only by default (dry-run). \
                Set confirm=true to actually apply the fix. Automatically reverts if compilation breaks.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "proposal_id": {
                        "type": "string",
                        "description": "The UUID of the proposal to apply (from janitor results)"
                    },
                    "confirm": {
                        "type": "boolean",
                        "description": "Set to true to actually apply the fix. Default: false (preview only).",
                        "default": false
                    }
                },
                "required": ["proposal_id"]
            }),
            annotations: rw(false),
        },
        ToolDefinition {
            name: "approve-fix".into(),
            description: "Apply a RepairOrchestrator auto-fix proposal by UUID. Preview-only by default (dry-run). \
                Set confirm=true to actually apply the fix. The fix is verified with `cargo check` and \
                auto-reverted on failure. Part of the Human-AI Collaborative Loop.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "proposal_id": {
                        "type": "string",
                        "description": "The UUID of the auto-repair proposal to apply"
                    },
                    "confirm": {
                        "type": "boolean",
                        "description": "Set to true to actually apply the fix. Default: false (preview only).",
                        "default": false
                    }
                },
                "required": ["proposal_id"]
            }),
            annotations: rw(false),
        },
        ToolDefinition {
            name: "architect".into(),
            description: "SPECIALIZED — Structural health analysis: dependency graph, coupling metrics, cycle detection, \
                god objects, layer violations. Returns architecture score (A-F) with violations and recommendations.".into(),
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
            annotations: ro(),
        },
        ToolDefinition {
            name: "oracle".into(),
            description: "SPECIALIZED — Auto-repair drifted documentation. Updates version numbers, crate counts, and MCP \
                tool/resource counts in README.md to match the actual codebase.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
            annotations: rw(false),
        },
        ToolDefinition {
            name: "reset-telemetry".into(),
            description: "Clear all telemetry data (spans and metrics) from the OTLP receiver. Use to reset the performance \
                heatmap and start fresh observation.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
            annotations: rw(false),
        },

        // ════════════════════════════════════════════════════════
        // SPECIALIZED — Neural Decompiler (binary analysis)
        // ════════════════════════════════════════════════════════
        #[cfg(feature = "decompiler")]
        ToolDefinition {
            name: "analyze_binary".into(),
            description: "SPECIALIZED — Analyze a compiled binary (ELF/Mach-O/PE). Extracts symbols, strings, call graph, \
                and infers behavioral patterns (network I/O, crypto, file I/O, etc.). Use to understand compiled dependencies \
                or audit third-party binaries without source code.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute or project-relative path to the binary file to analyze"
                    }
                },
                "required": ["path"]
            }),
            annotations: ro(),
        },
        #[cfg(feature = "decompiler")]
        ToolDefinition {
            name: "explain_dependency".into(),
            description: "SPECIALIZED — Explain what a compiled Rust dependency does by analyzing its built artifact \
                in target/debug/deps or target/release/deps. Finds the library by crate name and runs full binary analysis.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "crate_name": {
                        "type": "string",
                        "description": "Name of the dependency crate to explain (e.g., 'serde', 'tokio')"
                    }
                },
                "required": ["crate_name"]
            }),
            annotations: ro(),
        },

        // ════════════════════════════════════════════════════════
        // SPECIALIZED — Benchmark Engine (SCR evaluation)
        // ════════════════════════════════════════════════════════
        #[cfg(feature = "bench")]
        ToolDefinition {
            name: "run_benchmark".into(),
            description: "SPECIALIZED — Run a reproducible benchmark suite against the `ask` orchestrator. \
                Loads a JSONL question suite (question + ground_truth + difficulty), invokes `ask` for each question \
                via direct Rust API, scores responses, and returns a report with F1, SCR (Semantic Compression Ratio), \
                SID correlation, precision, recall, and hallucination rate.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "suite_path": {
                        "type": "string",
                        "description": "Path to a JSONL question suite file (absolute, project-relative, or filename in crates/bench/suites/)"
                    },
                    "format": {
                        "type": "string",
                        "description": "Output format: 'summary' (default, markdown + JSON) or 'json' (raw JSON report)",
                        "enum": ["summary", "json"],
                        "default": "summary"
                    }
                },
                "required": ["suite_path"]
            }),
            annotations: ro(),
        },
    ]
}

// ── Dispatch table ──────────────────────────────────────────────────

/// Canonical tool names (short, CLI-aligned).
const TOOL_NAMES: &[&str] = &[
    "hoist", "lookup", "scan", "check", "blame", "diagnose", "consult",
    "search", "diagnostics", "analyze", "quickfix", "ask", "intent",
    "train", "reset-telemetry", "janitor", "janitor-fix", "approve-fix",
    "architect", "oracle", "similar", "verify_path",
    #[cfg(feature = "decompiler")]
    "analyze_binary",
    #[cfg(feature = "decompiler")]
    "explain_dependency",
    #[cfg(feature = "bench")]
    "run_benchmark",
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
        "approve-fix" => Some("approve-fix"),
        "architect" => Some("architect"),
        "oracle" => Some("oracle"),
        "similar" => Some("similar"),
        "verify_path" => Some("verify_path"),
        #[cfg(feature = "decompiler")]
        "analyze_binary" => Some("analyze_binary"),
        #[cfg(feature = "decompiler")]
        "explain_dependency" => Some("explain_dependency"),
        "run_benchmark" => Some("run_benchmark"),
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
        "approve_fix" | "approve_repair" => Some("approve-fix"),
        "architect_analyze" => Some("architect"),
        "oracle_fix_docs" => Some("oracle"),
        "semantic_similarity" => Some("similar"),
        #[cfg(feature = "decompiler")]
        "decompile" | "neural_decompiler" => Some("analyze_binary"),
        "benchmark" | "bench" | "eval" | "evaluate" => Some("run_benchmark"),
        _ => None,
    }
}

/// Execute the handler for a resolved canonical tool name.
/// Wrapped in `catch_unwind` so a panic in any tool (tree-sitter, git2, …)
/// cannot kill the MCP server process (Q18 hardening).
fn dispatch_tool(
    canonical: &str,
    args: &serde_json::Value,
    ctx: &SynapseContext,
) -> ToolCallResult {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        dispatch_tool_inner(canonical, args, ctx)
    }));
    match result {
        Ok(tool_result) => tool_result,
        Err(panic) => {
            let msg = if let Some(s) = panic.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            error!(tool = canonical, panic = %msg, "Tool panicked — caught by catch_unwind");
            error_result(format!("Internal error in tool '{canonical}': {msg}"))
        }
    }
}

/// Inner dispatch — actual match on canonical tool name.
fn dispatch_tool_inner(
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
        "approve-fix" => approve_fix::tool_approve_fix(args, ctx),
        "architect" => architect::tool_architect_analyze(args, ctx),
        "oracle" => oracle::tool_oracle_fix_docs(ctx),
        "similar" => search::tool_semantic_similarity(args, ctx),
        "verify_path" => verify::tool_verify_path(args, ctx),
        #[cfg(feature = "decompiler")]
        "analyze_binary" => decompile::tool_analyze_binary(args, ctx),
        #[cfg(feature = "decompiler")]
        "explain_dependency" => decompile::tool_explain_dependency(args, ctx),
        #[cfg(feature = "bench")]
        "run_benchmark" => bench::tool_run_benchmark(args, ctx),
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
        if best.is_none_or(|(_, d)| dist < d) {
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
