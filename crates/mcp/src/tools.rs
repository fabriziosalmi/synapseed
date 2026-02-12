//! MCP Tool definitions and handlers.
//!
//! Each tool maps to an internal SYNAPSEED capability.

use serde_json::json;
use tracing::info;

use synapseed_core::context::SynapseContext;
use synapseed_cortex::graph::CodeGraph;
use synapseed_husk::guard::SecurityGuard;
use synapseed_root::sentinel::Sentinel;
use synapseed_chronos::historian::Historian;
use synapseed_core::state::ProjectState;
use synapseed_search::indexer::SemanticIndex;
use synapseed_shadow_check::runner::DiagnosticStore;
use synapseed_telemetry_sink::store::SpanStore;

use crate::protocol::{ToolDefinition, ToolCallResult, ContentBlock};

/// Return all available tool definitions.
pub fn list_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "get_code_skeleton".into(),
            description: "Index a project directory and return its AST skeleton (files, symbols, structure). Use this to understand the architecture before diving into code.".into(),
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
            description: "Find a symbol (function, class, struct, etc.) by name across the entire project. Returns file path, line numbers, and signature.".into(),
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
            description: "Scan text content for sensitive data (API keys, passwords, tokens, PII). Returns findings or CLEAN status.".into(),
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
            description: "Evaluate a shell command against the security policy. Returns ALLOWED or DENIED with reason.".into(),
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
            description: "Get git blame/history for a file. Shows who changed what and why — useful for understanding code context.".into(),
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
            description: "Run a full diagnostic on the project: detect state (virgin/partial/healthy), build system, git status, active plugins.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDefinition {
            name: "consult_architect".into(),
            description: "Consult the project's architecture policy. Returns guidance from the DNA configuration on preferred libraries, workspace strategy, naming conventions, and design principles.".into(),
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
            description: "Search for code by concept, not just exact strings. Finds symbols based on names, doc comments, signatures, and body content. Supports fuzzy matching (e.g., 'auth~2' for typo tolerance). Use this when you need to find code related to a concept like 'authentication', 'logging', or 'error handling'.".into(),
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
            description: "Get current compiler diagnostics (errors and warnings) from the background shadow compiler. Optionally filter by file path. Use this to check if the code compiles before proceeding.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file": {
                        "type": "string",
                        "description": "Optional file path to filter diagnostics (returns all if omitted)"
                    }
                }
            }),
        },
        ToolDefinition {
            name: "analyze_history".into(),
            description: "Analyze the full history of a file: churn/hotspot score, co-change patterns, semantic commit classification (fix/revert/refactor/security), and risk assessment. Optionally scope to a line range. Use this when asked 'Why is this code so complex?' or 'Is this area risky?'.".into(),
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
            description: "Apply a compiler-suggested fix automatically. Only applies 'MachineApplicable' suggestions from rustc. Use get_diagnostics first to find the error code, then apply the fix.".into(),
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
            name: "ask_whisperer".into(),
            description: "The Intent Router — ask a natural-language question and SYNAPSEED automatically orchestrates all relevant subsystems (compiler, search, history, security) in a single call. Returns an enriched context object with diagnostics, history, code context, and security status. Use this FIRST for any complex question instead of calling individual tools.".into(),
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
            description: "Summarize the intent and direction of recent commits semantically. Groups commits by category (fix, feature, refactor, security, etc.) and extracts scope hints from conventional commit messages. Use this to quickly understand what the team has been working on.".into(),
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
            name: "reset_telemetry".into(),
            description: "Clear all telemetry data (spans and metrics) from the OTLP receiver. Use this to reset the heatmap and start fresh observation.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {}
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
        "ask_whisperer" => tool_ask_whisperer(args, ctx),
        "git_intent_summary" => tool_git_intent_summary(args, ctx),
        "reset_telemetry" => tool_reset_telemetry(ctx),
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

    // Try shared graph from CortexPlugin for project root
    if path == root {
        if let Some(graph) = ctx.get_extension::<CodeGraph>() {
            let summary = json!({
                "files_indexed": graph.file_count(),
                "symbols_indexed": graph.symbol_count(),
                "path": path.display().to_string(),
            });
            return text_result(serde_json::to_string_pretty(&summary).unwrap_or_default());
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
fn get_historian(ctx: &SynapseContext) -> std::result::Result<std::sync::Arc<Historian>, ToolCallResult> {
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
        ProjectState::HealthyWorkspace { build_system, file_count } => {
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
        dna.preferred_libs.get("async").map(|s| s.as_str()).unwrap_or("tokio"),
        dna.preferred_libs.get("error").map(|s| s.as_str()).unwrap_or("thiserror"),
        dna.preferred_libs.get("json").map(|s| s.as_str()).unwrap_or("serde_json"),
        dna.dlp_level,
    );

    text_result(policy)
}

fn tool_semantic_search(args: &serde_json::Value, ctx: &SynapseContext) -> ToolCallResult {
    let query = match args.get("query").and_then(|v| v.as_str()) {
        Some(q) => q,
        None => return error_result("Missing required parameter: query".into()),
    };
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(5) as usize;

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
    let start_line = args.get("start_line").and_then(|v| v.as_u64()).map(|v| v as usize);
    let end_line = args.get("end_line").and_then(|v| v.as_u64()).map(|v| v as usize);

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
    let store = match ctx.get_extension::<DiagnosticStore>() {
        Some(s) => s,
        None => return text_result("Shadow compiler not active (no Cargo.toml found or not initialized). Run `synapseed init` first.".into()),
    };

    let file_filter = args.get("file").and_then(|v| v.as_str());

    let diagnostics = match file_filter {
        Some(file) => store.for_file(file),
        None => store.snapshot().diagnostics,
    };

    if diagnostics.is_empty() {
        let snap = store.snapshot();
        text_result(format!(
            "CLEAN: No diagnostics. Last check took {}ms.",
            snap.last_check_ms
        ))
    } else {
        let snap = store.snapshot();
        let json = serde_json::to_string_pretty(&diagnostics).unwrap_or_default();
        text_result(format!(
            "{} error(s), {} warning(s) (check took {}ms):\n{json}",
            snap.error_count, snap.warning_count, snap.last_check_ms
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

fn tool_ask_whisperer(args: &serde_json::Value, ctx: &SynapseContext) -> ToolCallResult {
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
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(20) as usize;

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
