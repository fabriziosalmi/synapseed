//! MCP Server — async JSON-RPC 2.0 over stdin/stdout.
//!
//! The main event loop reads JSON-RPC messages from stdin, dispatches them
//! (CPU-bound tool calls are offloaded via `spawn_blocking`), and writes
//! responses to stdout. All tracing/logging MUST go to stderr.

use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, error, info, warn};

use synapseed_core::context::SynapseContext;
use synapseed_core::momentum::{ModelTier, MomentumEngine};
use synapseed_core::session::SessionState;
use synapseed_core::state::ProjectState;

use crate::prompts;
use crate::protocol::*;
use crate::resources;
use crate::tools;

/// Run the MCP server on stdin/stdout (async).
///
/// CPU-bound tool calls are offloaded to the blocking thread pool
/// via `tokio::task::spawn_blocking`, keeping the main loop responsive.
pub async fn run(ctx: SynapseContext) -> anyhow::Result<()> {
    info!("MCP server starting (async stdio transport)");

    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut lines = BufReader::new(stdin).lines();
    let mut initialized = false;
    let token = ctx.shutdown_token();

    loop {
        let line = tokio::select! {
            result = lines.next_line() => {
                match result? {
                    Some(line) => line,
                    None => break, // EOF
                }
            }
            _ = token.cancelled() => {
                info!("MCP server: shutdown signal received");
                break;
            }
        };

        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        // Try to parse as a request (has "id")
        if let Ok(request) = serde_json::from_str::<JsonRpcRequest>(&line) {
            let response = handle_request(&request, &ctx, &mut initialized).await;
            let json = serde_json::to_string(&response).unwrap_or_default();
            stdout.write_all(json.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
            continue;
        }

        // Try to parse as a notification (no "id")
        if let Ok(notification) = serde_json::from_str::<JsonRpcNotification>(&line) {
            handle_notification(&notification, &ctx, &mut initialized);
            continue;
        }

        // Unparseable — send error response
        let response = JsonRpcResponse::error(
            serde_json::Value::Null,
            PARSE_ERROR,
            "Parse error: invalid JSON-RPC message".into(),
        );
        let json = serde_json::to_string(&response).unwrap_or_default();
        stdout.write_all(json.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }

    // HCI Req 9: Save session state on shutdown
    let metrics = ctx.metrics();
    let root = ctx.project_root();
    let mut session = SessionState::new(&root);
    session.files_indexed = metrics.files_indexed;
    session.tools_invoked = metrics.tools_invoked;
    session.save(&root);

    info!("MCP server shutting down");
    Ok(())
}

async fn handle_request(
    req: &JsonRpcRequest,
    ctx: &SynapseContext,
    initialized: &mut bool,
) -> JsonRpcResponse {
    info!(method = %req.method, "MCP: Request");

    // Gate: reject all methods before initialization (except initialize itself)
    if !*initialized && req.method != "initialize" {
        return JsonRpcResponse::error(
            req.id.clone(),
            INVALID_REQUEST,
            "Server not initialized. Send 'initialize' first.".into(),
        );
    }

    match req.method.as_str() {
        // ── MCP Lifecycle ──────────────────────────────────
        "initialize" => handle_initialize(req, ctx),

        // ── Tools ──────────────────────────────────────────
        "tools/list" => {
            let tools = tools::list_tools();
            JsonRpcResponse::success(req.id.clone(), json!({ "tools": tools }))
        }
        "tools/call" => {
            // Offload CPU-bound tool execution to blocking thread pool
            let ctx_cloned = ctx.clone();
            let id = req.id.clone();
            let name = req
                .params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let arguments = req.params.get("arguments").cloned().unwrap_or(json!({}));
            let name_for_momentum = name.clone();

            match tokio::task::spawn_blocking(move || {
                tools::handle_tool_call(&name, &arguments, &ctx_cloned)
            })
            .await
            {
                Ok(result) => {
                    // HCI Req 9: Track tool invocations for session continuity
                    ctx.update_metrics(|m| m.tools_invoked += 1);
                    // Momentum Engine: record tool for phase detection (#52)
                    if let Some(engine) = ctx.get_extension::<Mutex<MomentumEngine>>() {
                        engine.lock().record_tool(&name_for_momentum);
                    }
                    JsonRpcResponse::success(id, serde_json::to_value(result).unwrap_or_default())
                }
                Err(e) => {
                    error!(error = %e, "Tool task panicked");
                    JsonRpcResponse::error(
                        id,
                        INTERNAL_ERROR,
                        format!("Tool execution failed: {e}"),
                    )
                }
            }
        }

        // ── Resources ──────────────────────────────────────
        "resources/list" => {
            let resources = resources::list_resources();
            JsonRpcResponse::success(req.id.clone(), json!({ "resources": resources }))
        }
        "resources/read" => {
            let uri = req.params.get("uri").and_then(|v| v.as_str()).unwrap_or("");
            match resources::read_resource(uri, ctx) {
                Some(content) => {
                    JsonRpcResponse::success(req.id.clone(), json!({ "contents": [content] }))
                }
                None => JsonRpcResponse::error(
                    req.id.clone(),
                    INVALID_PARAMS,
                    format!("Unknown resource: {uri}"),
                ),
            }
        }

        // ── Prompts ────────────────────────────────────────
        "prompts/list" => {
            let prompts = prompts::list_prompts();
            JsonRpcResponse::success(req.id.clone(), json!({ "prompts": prompts }))
        }
        "prompts/get" => {
            let name = req
                .params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let arguments = req.params.get("arguments").cloned().unwrap_or(json!({}));
            match prompts::get_prompt(name, &arguments) {
                Some(messages) => {
                    JsonRpcResponse::success(req.id.clone(), json!({ "messages": messages }))
                }
                None => JsonRpcResponse::error(
                    req.id.clone(),
                    INVALID_PARAMS,
                    format!("Unknown prompt: {name}"),
                ),
            }
        }

        // ── Ping ───────────────────────────────────────────
        "ping" => JsonRpcResponse::success(req.id.clone(), json!({})),

        // ── Unknown method ─────────────────────────────────
        _ => {
            warn!(method = %req.method, "MCP: Unknown method");
            JsonRpcResponse::error(
                req.id.clone(),
                METHOD_NOT_FOUND,
                format!("Method not found: {}", req.method),
            )
        }
    }
}

fn handle_notification(notif: &JsonRpcNotification, _ctx: &SynapseContext, initialized: &mut bool) {
    info!(method = %notif.method, "MCP: Notification");

    match notif.method.as_str() {
        "notifications/initialized" => {
            *initialized = true;
            info!("MCP: Client confirmed initialization — server is live");
        }
        "notifications/cancelled" => {
            info!("MCP: Client cancelled a request");
        }
        _ => {
            warn!(method = %notif.method, "MCP: Unknown notification");
        }
    }
}

fn handle_initialize(req: &JsonRpcRequest, ctx: &SynapseContext) -> JsonRpcResponse {
    info!("MCP: Initializing server");

    // ── Client Fingerprinting (v3.6.2 #53) ──────────────────────
    let dna = ctx.dna();
    let tier = if let Some(profile) = &dna.hci.model_profile {
        // DNA override always wins
        let t = ModelTier::from_config(profile).unwrap_or_default();
        info!(tier = %t, source = "dna", "Model tier from DNA override");
        t
    } else if let Some(client_info) = req.params.get("clientInfo") {
        let client_name = client_info
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("unknown");
        let t = ModelTier::from_client_name(client_name);
        info!(tier = %t, client = client_name, "Model tier from client fingerprint");
        t
    } else {
        debug!("No clientInfo in initialize, defaulting to Molecular");
        ModelTier::default()
    };

    // Register MomentumEngine in the extension registry
    let engine = MomentumEngine::new(tier);
    ctx.set_extension(Arc::new(Mutex::new(engine)));

    // Dynamic Context Injection based on project state
    let instructions = build_instructions(ctx);

    let result = InitializeResult {
        protocol_version: "2024-11-05".into(),
        capabilities: ServerCapabilities {
            tools: Some(ToolsCapability {
                list_changed: Some(false),
            }),
            resources: Some(ResourcesCapability {
                subscribe: Some(false),
                list_changed: Some(false),
            }),
            prompts: Some(PromptsCapability {
                list_changed: Some(false),
            }),
            logging: Some(json!({})),
        },
        server_info: ServerInfo {
            name: "synapseed".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        },
        instructions: Some(instructions),
    };

    JsonRpcResponse::success(
        req.id.clone(),
        serde_json::to_value(result).unwrap_or_default(),
    )
}

/// Build context-aware instructions injected into the LLM on initialization.
///
/// This is the "Dynamic Context Injection" — SYNAPSEED detects the project
/// state and tells the LLM exactly what to do.
fn build_instructions(ctx: &SynapseContext) -> String {
    let state = ctx.project_state();
    let dna = ctx.dna();

    let mut instructions = String::from(
        "You are connected to SYNAPSEED, a high-performance semantic AI middleware. \
         You have access to AST-based code understanding, DLP security scanning, \
         command sandboxing, and Git time-travel capabilities.\n\n",
    );

    match &state {
        ProjectState::VirginRepo => {
            instructions.push_str(
                "⚠️ VIRGIN REPOSITORY DETECTED — This project has no build system or code structure.\n\n\
                 RECOMMENDED WORKFLOW:\n\
                 1. Ask the user what kind of project they want to create\n\
                 2. Use `project_diagnose` to confirm the current state\n\
                 3. Suggest a project scaffold based on their language/framework choice\n\
                 4. After scaffolding, use `get_code_skeleton` to verify the structure\n\n\
                 Available tools: get_code_skeleton, lookup_symbol, scan_security, check_command, git_history, project_diagnose, consult_architect\n",
            );
        }
        ProjectState::PartialSetup { missing, .. } => {
            instructions.push_str(&format!(
                "⚠️ PARTIAL PROJECT SETUP — The project exists but is incomplete.\n\
                 Missing: {}\n\n\
                 RECOMMENDED WORKFLOW:\n\
                 1. Use `project_diagnose` to see full diagnostic\n\
                 2. Use `get_code_skeleton` to understand what exists\n\
                 3. Help complete the missing components\n\
                 4. Use `scan_security` on any config files before committing\n",
                missing.join(", ")
            ));
        }
        ProjectState::HealthyWorkspace {
            build_system,
            file_count,
        } => {
            instructions.push_str(&format!(
                "✅ HEALTHY WORKSPACE — Build system: {build_system:?}, Files: {file_count}\n\n\
                 RECOMMENDED WORKFLOW:\n\
                 1. Start with `get_code_skeleton` for architecture overview\n\
                 2. Use `lookup_symbol` to find specific types/functions\n\
                 3. Use `git_history` to understand code evolution\n\
                 4. Use `consult_architect` to check architecture policy before making structural decisions\n\
                 5. ALWAYS use `scan_security` before outputting code containing config or credentials\n\
                 6. Use `check_command` before suggesting shell commands to the user\n",
            ));
        }
        ProjectState::Unknown => {
            instructions.push_str(
                "❓ UNKNOWN PROJECT TYPE — Could not detect build system.\n\n\
                 RECOMMENDED: Run `project_diagnose` first to understand the project.\n",
            );
        }
    }

    // Append DNA context
    instructions.push_str(&format!(
        "\nProject DNA:\n- Strategy: {}\n- DLP Level: {:?}\n- Plugins: {}\n",
        dna.workspace_strategy,
        dna.dlp_level,
        dna.plugins.join(", "),
    ));

    // Momentum Engine: inject model tier if available
    if let Some(engine) = ctx.get_extension::<Mutex<MomentumEngine>>() {
        let e = engine.lock();
        instructions.push_str(&format!("- Model Tier: {}\n", e.tier()));
    }

    // HCI Req 9 (Time Anchor): inject session continuity if recent session exists
    let root = ctx.project_root();
    if let Some(session) = SessionState::load(&root) {
        if session.is_recent() {
            instructions.push_str(&format!(
                "\nSESSION CONTINUITY: Last session was {}. {} files indexed, {} tools used. Resuming context.\n",
                session.time_ago(),
                session.files_indexed,
                session.tools_invoked,
            ));
        }
    }

    instructions
}
