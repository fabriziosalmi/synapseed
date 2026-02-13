#![forbid(unsafe_code)]
use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Parser, Subcommand};
use serde_json::json;
use tracing::info;

use synapseed_core::context::SynapseContext;
use synapseed_core::event::SynapseEvent;
use synapseed_core::liquid::ProjectDna;
use synapseed_core::plugin::SynapsePlugin;
use synapseed_core::state::ProjectState;
use synapseed_core::telemetry;

use synapseed_chronos::plugin::ChronosPlugin;
use synapseed_cortex::graph::CodeGraph;
use synapseed_cortex::plugin::CortexPlugin;
use synapseed_husk::plugin::HuskPlugin;
use synapseed_root::plugin::RootPlugin;
use synapseed_root::sentinel::Sentinel;
use synapseed_search::plugin::SearchPlugin;
use synapseed_shadow_check::plugin::ShadowCheckPlugin;
use synapseed_telemetry_sink::plugin::TelemetrySinkPlugin;
use synapseed_visualizer::plugin::VisualizerPlugin;
use synapseed_gym::plugin::GymPlugin;
use synapseed_janitor::plugin::JanitorPlugin;
use synapseed_architect::plugin::ArchitectPlugin;
use synapseed_whisper::plugin::WhisperPlugin;

use synapseed_mcp::protocol::ContentBlock;
use synapseed_mcp::tools::handle_tool_call;

#[derive(Parser)]
#[command(
    name = "synapseed",
    about = "High-Performance Semantic AI Middleware — The Thinking Layer Between You and the LLM",
    version,
    after_help = "SYNAPSEED: Where intelligence meets infrastructure.\n\n\
        Every MCP tool is available as a CLI command. Legacy names (e.g. ask_synapseed, \
        get_code_skeleton) are accepted as aliases."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Project root directory
    #[arg(short, long, global = true, default_value = ".")]
    project: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    // ── Existing commands (with MCP aliases) ────────────────────────

    /// Index a project and display its code graph skeleton
    #[command(visible_alias = "get_code_skeleton")]
    Hoist {
        /// Directory to index (default: project root)
        path: Option<String>,
    },

    /// Look up a symbol by name across the indexed project
    #[command(visible_alias = "lookup_symbol")]
    Lookup {
        /// Symbol name to search for
        name: String,
    },

    /// Scan content for sensitive data (DLP check)
    #[command(visible_alias = "scan_security")]
    Scan {
        /// Content to scan (reads from stdin if not provided)
        #[arg(short, long)]
        content: Option<String>,
        /// Scan mode: all (default), dlp, patterns
        #[arg(short, long, default_value = "all")]
        mode: String,
    },

    /// Evaluate a command against the security sentinel
    #[command(visible_alias = "check_command")]
    Check {
        /// Command to evaluate
        command: String,
    },

    /// Run full system diagnostic — detect state, load plugins, report
    #[command(visible_alias = "project_diagnose")]
    Diagnose,

    /// Show git history summary and recent commits
    History {
        /// Number of recent commits to show
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },

    /// Show blame information for a file
    #[command(visible_alias = "git_history")]
    Blame {
        /// File path (relative to project root)
        file: String,
        /// Start line
        #[arg(short, long, default_value = "1")]
        start: usize,
        /// End line
        #[arg(short, long, default_value = "20")]
        end: usize,
    },

    /// Show runtime metrics and system status
    Status,

    /// Initialize all plugins and broadcast SystemInit event
    Init,

    /// Start MCP server (JSON-RPC 2.0 over stdio) — connect to Claude Desktop
    Serve,

    // ── New commands (MCP-only tools exposed to CLI) ────────────────

    /// Ask a natural-language question — SYNAPSEED orchestrates all subsystems
    #[command(visible_alias = "ask_synapseed", visible_alias = "whisper")]
    Ask {
        /// Natural-language question
        query: String,
        /// Inject raw source code of discovered symbols into the prompt
        #[arg(long)]
        raw: bool,
    },

    /// Search for code by concept (Tantivy keyword index)
    #[command(visible_alias = "semantic_search")]
    Search {
        /// Search query
        query: String,
        /// Maximum number of results
        #[arg(short, long, default_value = "5")]
        limit: usize,
    },

    /// Show compiler diagnostics from the shadow compiler
    #[command(visible_alias = "get_diagnostics")]
    Diagnostics {
        /// Filter by file path
        #[arg(short, long)]
        file: Option<String>,
        /// Minimum severity: info, warning, error
        #[arg(short, long, default_value = "warning")]
        min_severity: String,
    },

    /// Analyze file history: churn, hotspots, co-change patterns, risk
    #[command(visible_alias = "analyze_history")]
    Analyze {
        /// File path (relative to project root)
        file: String,
        /// Start line (optional scope)
        #[arg(short, long)]
        start: Option<usize>,
        /// End line (optional scope)
        #[arg(short, long)]
        end: Option<usize>,
    },

    /// Apply a compiler-suggested quick fix
    #[command(visible_alias = "apply_quick_fix")]
    Quickfix {
        /// File path containing the error
        file: String,
        /// Error/warning code (e.g. unused_variables, E0425)
        error_code: String,
    },

    /// Summarize the intent of recent commits semantically
    #[command(visible_alias = "git_intent_summary")]
    Intent {
        /// Number of recent commits to analyze
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    /// Evaluate Rust code in the Gym sandbox
    #[command(visible_alias = "train_code")]
    Train {
        /// Path to Rust source file (or - for stdin)
        source: String,
        /// Path to test file
        #[arg(short, long)]
        tests: Option<String>,
        /// Timeout in seconds
        #[arg(long, default_value = "60")]
        timeout: usize,
        /// Enable proptest fuzzing
        #[arg(long)]
        fuzz: bool,
        /// Enable adversarial mutation testing
        #[arg(long)]
        adversarial: bool,
    },

    /// Clear all telemetry data (OTLP spans and metrics)
    #[command(visible_alias = "reset_telemetry")]
    ResetTelemetry,

    /// Run Janitor: scan clippy warnings and unused dependencies
    #[command(visible_alias = "janitor_run_now")]
    Janitor,

    /// Apply a Janitor fix proposal by ID
    #[command(visible_alias = "janitor_apply_fix")]
    JanitorFix {
        /// UUID of the proposal to apply
        proposal_id: String,
        /// Actually apply (default: preview only)
        #[arg(long)]
        confirm: bool,
    },

    /// Analyze project structural health (architecture score, coupling, cycles)
    #[command(visible_alias = "architect_analyze")]
    Architect {
        /// Force fresh analysis (skip cache)
        #[arg(long)]
        refresh: bool,
    },

    /// Consult the project's architecture policy (DNA config)
    #[command(visible_alias = "consult_architect")]
    Consult {
        /// Architecture question
        query: String,
    },

    /// Auto-repair drifted documentation (versions, counts)
    #[command(visible_alias = "oracle_fix_docs")]
    Oracle,

    /// Find code similar to a query using vector embeddings
    #[command(visible_alias = "semantic_similarity")]
    Similar {
        /// Natural-language query
        query: String,
        /// Number of results
        #[arg(short = 'k', long, default_value = "5")]
        top_k: usize,
        /// Minimum cosine similarity threshold
        #[arg(short, long, default_value = "0.3")]
        min_similarity: f64,
    },

    /// Catch-all: unrecognized input is treated as an `ask` query
    #[command(external_subcommand)]
    External(Vec<String>),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // MCP serve mode: logs MUST go to stderr (stdout = JSON-RPC transport)
    let is_serve = matches!(cli.command, Commands::Serve);
    if is_serve {
        telemetry::init_telemetry_stderr();
    } else {
        telemetry::init_telemetry();
    }

    let project_root = std::fs::canonicalize(&cli.project)?;

    match cli.command {
        // ── Existing commands ───────────────────────────────────────
        Commands::Hoist { path } => {
            let mut args = json!({});
            if let Some(p) = path {
                args["path"] = json!(p);
            }
            cmd_mcp(&project_root, "hoist", args).await?
        }
        Commands::Lookup { name } => cmd_lookup(&name, &project_root)?,
        Commands::Scan { content, mode } => {
            let text = match content {
                Some(c) => c,
                None => {
                    use std::io::Read;
                    let mut buf = String::new();
                    std::io::stdin().read_to_string(&mut buf)?;
                    buf
                }
            };
            cmd_mcp(&project_root, "scan", json!({"content": text, "mode": mode})).await?
        }
        Commands::Check { command } => cmd_check(&command)?,
        Commands::Diagnose => cmd_diagnose(&project_root)?,
        Commands::History { limit } => cmd_history(&project_root, limit)?,
        Commands::Blame { file, start, end } => cmd_blame(&project_root, &file, start, end)?,
        Commands::Status => cmd_status(&project_root).await?,
        Commands::Init => cmd_init(&project_root).await?,
        Commands::Serve => cmd_serve(&project_root).await?,

        // ── MCP-bridged commands ────────────────────────────────────
        Commands::Ask { query, raw } => {
            cmd_ask(&project_root, &query, raw).await?
        }
        Commands::Search { query, limit } => {
            cmd_mcp(&project_root, "search", json!({"query": query, "limit": limit})).await?
        }
        Commands::Diagnostics { file, min_severity } => {
            let mut args = json!({"min_severity": min_severity});
            if let Some(f) = file {
                args["file"] = json!(f);
            }
            cmd_mcp(&project_root, "diagnostics", args).await?
        }
        Commands::Analyze { file, start, end } => {
            let mut args = json!({"file": file});
            if let Some(s) = start { args["start_line"] = json!(s); }
            if let Some(e) = end { args["end_line"] = json!(e); }
            cmd_mcp(&project_root, "analyze", args).await?
        }
        Commands::Quickfix { file, error_code } => {
            cmd_mcp(&project_root, "quickfix", json!({"file": file, "error_code": error_code})).await?
        }
        Commands::Intent { limit } => {
            cmd_mcp(&project_root, "intent", json!({"limit": limit})).await?
        }
        Commands::Train { source, tests, timeout, fuzz, adversarial } => {
            let src = read_source_or_stdin(&source)?;
            let mut args = json!({"source": src, "timeout": timeout, "fuzz": fuzz, "adversarial": adversarial});
            if let Some(t) = tests {
                args["tests"] = json!(std::fs::read_to_string(&t)?);
            }
            cmd_mcp(&project_root, "train", args).await?
        }
        Commands::ResetTelemetry => {
            cmd_mcp(&project_root, "reset-telemetry", json!({})).await?
        }
        Commands::Janitor => {
            cmd_mcp(&project_root, "janitor", json!({})).await?
        }
        Commands::JanitorFix { proposal_id, confirm } => {
            cmd_mcp(&project_root, "janitor-fix", json!({"proposal_id": proposal_id, "confirm": confirm})).await?
        }
        Commands::Architect { refresh } => {
            cmd_mcp(&project_root, "architect", json!({"refresh": refresh})).await?
        }
        Commands::Consult { query } => {
            cmd_mcp(&project_root, "consult", json!({"query": query})).await?
        }
        Commands::Oracle => {
            cmd_mcp(&project_root, "oracle", json!({})).await?
        }
        Commands::Similar { query, top_k, min_similarity } => {
            cmd_mcp(&project_root, "similar", json!({"query": query, "top_k": top_k, "min_similarity": min_similarity})).await?
        }
        Commands::External(args) => {
            let query = args.join(" ");
            eprintln!("[ask] {query}");
            cmd_ask(&project_root, &query, false).await?
        }
    }

    Ok(())
}

// ── Shared helpers ──────────────────────────────────────────────────

/// Build a fully-initialized SynapseContext with all plugins (same as serve mode).
async fn init_full_context(path: &Path) -> Result<SynapseContext> {
    let state = ProjectState::detect(path);
    let dna = ProjectDna::load(path);
    let ctx = SynapseContext::new(path.to_path_buf(), state, dna.clone());

    let mut plugins: Vec<Box<dyn SynapsePlugin>> = vec![
        Box::new(HuskPlugin::from_dna(&dna)),
        Box::new(RootPlugin::new()),
        Box::new(CortexPlugin::new()),
        Box::new(ChronosPlugin::new()),
        Box::new(ShadowCheckPlugin::new()),
        Box::new(SearchPlugin::new()),
        Box::new(TelemetrySinkPlugin::new()),
        Box::new(VisualizerPlugin::from_config(&dna)),
        Box::new(ArchitectPlugin::new()),
        Box::new(WhisperPlugin::new()),
        Box::new(GymPlugin::new()),
        Box::new(JanitorPlugin::new()),
    ];
    plugins.sort_by_key(|p| p.priority());

    for plugin in &mut plugins {
        if let Err(e) = plugin.on_init(&ctx) {
            eprintln!("[WARN] {} failed to init: {e}", plugin.name());
        }
    }

    Ok(ctx)
}

/// `ask` with auto-hoist: ensure the code graph is fully indexed before querying.
///
/// In CLI mode the Cortex plugin indexes in a background thread, so the graph
/// may still be empty when the Whisperer processes the query.  This function
/// detects that situation and performs a **synchronous** hoist (equivalent to
/// `synapseed hoist . && synapseed ask "..."`) in a single process.
async fn cmd_ask(path: &Path, query: &str, raw: bool) -> Result<()> {
    let ctx = init_full_context(path).await?;

    // Wait briefly for the background indexer, then fall back to synchronous.
    if let Some(graph) = ctx.get_extension::<CodeGraph>() {
        // Give the background thread up to 500 ms to finish.
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        while graph.file_count() == 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(25));
        }

        if graph.file_count() == 0 {
            // Background indexing didn't finish in time — do it synchronously.
            info!("ask: auto-hoisting project (synchronous index)");
            if let Err(e) = graph.index_directory(path) {
                eprintln!("[WARN] auto-hoist failed: {e}");
            } else {
                ctx.update_metrics(|m| {
                    m.files_indexed = graph.file_count();
                    m.symbols_found = graph.symbol_count();
                });
                info!(
                    files = graph.file_count(),
                    symbols = graph.symbol_count(),
                    "ask: auto-hoist complete"
                );
            }
        }
    } else {
        // No graph registered at all — build one from scratch.
        info!("ask: no code graph found, auto-hoisting");
        let graph = std::sync::Arc::new(CodeGraph::new());
        if let Err(e) = graph.index_directory(path) {
            eprintln!("[WARN] auto-hoist failed: {e}");
        } else {
            ctx.update_metrics(|m| {
                m.files_indexed = graph.file_count();
                m.symbols_found = graph.symbol_count();
            });
            ctx.set_extension(graph);
        }
    }

    let result = handle_tool_call("ask", &json!({"query": query, "raw": raw}), &ctx);

    for block in &result.content {
        match block {
            ContentBlock::Text { text } => println!("{text}"),
        }
    }

    if result.is_error == Some(true) {
        std::process::exit(1);
    }

    Ok(())
}

/// Generic MCP tool bridge: init context, call tool, print result.
async fn cmd_mcp(path: &Path, tool: &str, args: serde_json::Value) -> Result<()> {
    let ctx = init_full_context(path).await?;
    let result = handle_tool_call(tool, &args, &ctx);

    for block in &result.content {
        match block {
            ContentBlock::Text { text } => println!("{text}"),
        }
    }

    if result.is_error == Some(true) {
        std::process::exit(1);
    }

    Ok(())
}

/// Read source from a file path, or from stdin if path is "-".
fn read_source_or_stdin(path: &str) -> Result<String> {
    if path == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        Ok(buf)
    } else {
        Ok(std::fs::read_to_string(path)?)
    }
}

// ── Existing command handlers ───────────────────────────────────────

fn cmd_lookup(name: &str, path: &Path) -> Result<()> {
    let graph = CodeGraph::new();
    graph.index_directory(path)?;

    let results = graph.lookup(name);

    if results.is_empty() {
        println!("No symbols found matching '{name}'");
    } else {
        println!("Found {} symbol(s):\n", results.len());
        for sym in &results {
            let json = serde_json::to_string_pretty(sym)?;
            println!("{json}\n");
        }
    }

    Ok(())
}

fn cmd_check(command: &str) -> Result<()> {
    let sentinel = Sentinel::with_defaults()?;

    match sentinel.evaluate(command) {
        Ok(action) => println!("ALLOWED ({action:?}): {command}"),
        Err(e) => println!("DENIED: {e}"),
    }

    Ok(())
}

fn cmd_diagnose(path: &Path) -> Result<()> {
    println!("=== SYNAPSEED SYSTEM DIAGNOSTIC ===\n");

    // State detection
    let state = ProjectState::detect(path);
    println!("{}\n", state.diagnostic());

    // DNA configuration
    let dna = ProjectDna::load(path);
    println!("--- DNA Configuration ---");
    println!("Workspace Strategy: {}", dna.workspace_strategy);
    println!("DLP Level: {:?}", dna.dlp_level);
    println!("Plugins: {}", dna.plugins.join(", "));
    println!(
        "Preferred Libs: {}",
        dna.preferred_libs
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(", ")
    );

    // Git state
    println!("\n--- Git Status ---");
    match synapseed_chronos::historian::Historian::open(path) {
        Ok(historian) => {
            let summary = historian.summary(3)?;
            println!(
                "Branch: {}",
                summary.branch.as_deref().unwrap_or("detached")
            );
            println!("HEAD: {}", summary.head_commit);
            println!("Commits: {}", summary.total_commits);
            println!("Dirty: {}", summary.is_dirty);
        }
        Err(e) => println!("Not a git repository: {e}"),
    }

    println!("\n=== DIAGNOSTIC COMPLETE ===");
    Ok(())
}

fn cmd_history(path: &Path, limit: usize) -> Result<()> {
    let historian = synapseed_chronos::historian::Historian::open(path)?;
    let summary = historian.summary(limit)?;

    println!(
        "Branch: {} | HEAD: {} | Dirty: {}\n",
        summary.branch.as_deref().unwrap_or("detached"),
        &summary.head_commit[..8.min(summary.head_commit.len())],
        summary.is_dirty
    );

    for commit in &summary.recent_commits {
        println!(
            "  {} | {} | {} | {}",
            commit.id, commit.timestamp, commit.author, commit.message
        );
    }

    println!("\nTotal commits: {}", summary.total_commits);
    Ok(())
}

fn cmd_blame(path: &Path, file: &str, start: usize, end: usize) -> Result<()> {
    let historian = synapseed_chronos::historian::Historian::open(path)?;
    let blame = historian.blame_lines(file, start, end)?;

    if blame.is_empty() {
        println!("No blame data for {file}:{start}-{end}");
    } else {
        println!("Blame for {file}:{start}-{end}\n");
        for entry in &blame {
            println!(
                "  L{:<4} | {} | {} | {} | {}",
                entry.line, entry.commit_id, entry.timestamp, entry.author, entry.message
            );
        }
    }

    Ok(())
}

async fn cmd_status(path: &Path) -> Result<()> {
    let state = ProjectState::detect(path);
    let dna = ProjectDna::load(path);
    let ctx = SynapseContext::new(path.to_path_buf(), state.clone(), dna);

    // Init all plugins to gather metrics
    let mut plugins: Vec<Box<dyn SynapsePlugin>> = vec![
        Box::new(HuskPlugin::new()),
        Box::new(CortexPlugin::new()),
        Box::new(RootPlugin::new()),
        Box::new(ChronosPlugin::new()),
    ];

    for plugin in &mut plugins {
        if let Err(e) = plugin.on_init(&ctx) {
            println!("  [WARN] {} failed to init: {e}", plugin.name());
        }
    }

    let metrics = ctx.metrics();

    println!("=== SYNAPSEED STATUS ===\n");
    println!("Project: {}", path.display());
    println!("State: {:?}\n", state);
    println!("--- Metrics ---");
    println!("Files Indexed:     {}", metrics.files_indexed);
    println!("Symbols Found:     {}", metrics.symbols_found);
    println!("DLP Scans:         {}", metrics.dlp_scans);
    println!("DLP Blocks:        {}", metrics.dlp_blocks);
    println!("Commands Allowed:  {}", metrics.commands_allowed);
    println!("Commands Denied:   {}", metrics.commands_denied);
    println!("Errors Prevented:  {}", metrics.errors_prevented);
    println!("Events Broadcast:  {}", metrics.events_broadcast);

    println!("\n--- Plugins ---");
    for plugin in &plugins {
        println!("  [OK] {} (priority: {})", plugin.name(), plugin.priority());
    }

    println!("\n=== STATUS COMPLETE ===");
    Ok(())
}

async fn cmd_init(path: &Path) -> Result<()> {
    let state = ProjectState::detect(path);
    let dna = ProjectDna::load(path);
    let ctx = SynapseContext::new(path.to_path_buf(), state.clone(), dna);

    println!("=== SYNAPSEED INIT ===\n");
    println!("{}\n", state.diagnostic());

    // Load and init plugins sorted by priority
    let mut plugins: Vec<Box<dyn SynapsePlugin>> = vec![
        Box::new(HuskPlugin::new()),
        Box::new(RootPlugin::new()),
        Box::new(CortexPlugin::new()),
        Box::new(ChronosPlugin::new()),
    ];

    // Sort by priority (lowest first = highest priority)
    plugins.sort_by_key(|p| p.priority());

    for plugin in &mut plugins {
        match plugin.on_init(&ctx) {
            Ok(()) => println!(
                "  [OK] {} initialized (priority: {})",
                plugin.name(),
                plugin.priority()
            ),
            Err(e) => println!("  [FAIL] {} error: {e}", plugin.name()),
        }
    }

    // Broadcast SystemInit event
    let event = SynapseEvent::SystemInit {
        project_root: path.display().to_string(),
        state: state.clone(),
    };

    let receivers = ctx.broadcast(event.clone());
    println!("\nBroadcast SystemInit -> {receivers} subscriber(s)");

    // Process the event through all plugins
    for plugin in &plugins {
        match plugin.on_event(&event, &ctx).await {
            Ok(Some(new_event)) => {
                println!(
                    "  [EVENT] {} emitted: {:?}",
                    plugin.name(),
                    std::mem::discriminant(&new_event)
                );
                ctx.broadcast(new_event);
            }
            Ok(None) => {}
            Err(e) => println!("  [WARN] {} event error: {e}", plugin.name()),
        }
    }

    let metrics = ctx.metrics();
    println!("\n--- Init Summary ---");
    println!(
        "Files: {} | Symbols: {} | Events: {}",
        metrics.files_indexed, metrics.symbols_found, metrics.events_broadcast
    );

    println!("\n=== SYNAPSEED READY ===");
    Ok(())
}

async fn cmd_serve(path: &Path) -> Result<()> {
    let state = ProjectState::detect(path);
    let dna = ProjectDna::load(path);
    let ctx = SynapseContext::new(path.to_path_buf(), state, dna.clone());

    // Initialize plugins before starting the server
    let mut plugins: Vec<Box<dyn SynapsePlugin>> = vec![
        Box::new(HuskPlugin::from_dna(&dna)),
        Box::new(RootPlugin::new()),
        Box::new(CortexPlugin::new()),
        Box::new(ChronosPlugin::new()),
        Box::new(ShadowCheckPlugin::new()),
        Box::new(SearchPlugin::new()),
        Box::new(TelemetrySinkPlugin::new()),
        Box::new(VisualizerPlugin::from_config(&dna)),
        Box::new(ArchitectPlugin::new()),
        Box::new(WhisperPlugin::new()),
        Box::new(GymPlugin::new()),
        Box::new(JanitorPlugin::new()),
    ];
    plugins.sort_by_key(|p| p.priority());

    for plugin in &mut plugins {
        if let Err(e) = plugin.on_init(&ctx) {
            eprintln!("[WARN] {} failed to init: {e}", plugin.name());
        }
    }

    info!("Starting MCP server on stdio");

    // Run MCP server — exits on stdin EOF or shutdown signal
    tokio::select! {
        result = synapseed_mcp::server::run(ctx.clone()) => {
            if let Err(e) = result {
                eprintln!("[ERROR] MCP server error: {e}");
            }
        }
        _ = tokio::signal::ctrl_c() => {
            eprintln!("[INFO] Received SIGINT");
        }
        _ = sigterm() => {
            eprintln!("[INFO] Received SIGTERM");
        }
    }

    // ── Graceful Shutdown Sequence ───────────────────────────────
    eprintln!("[INFO] Initiating graceful shutdown...");

    // 1. Signal all background tasks to stop
    ctx.request_shutdown();

    // 2. Broadcast SystemShutdown event to all subscribers
    ctx.broadcast(SynapseEvent::SystemShutdown);

    // 3. Call on_shutdown() on plugins in reverse priority order
    for plugin in plugins.iter().rev() {
        if let Err(e) = plugin.on_shutdown(&ctx) {
            eprintln!("[WARN] {} shutdown error: {e}", plugin.name());
        }
    }

    // 4. Brief grace period for background tasks to finish
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    eprintln!("[INFO] Shutdown complete");
    Ok(())
}

/// Wait for SIGTERM (Unix) or pend forever (Windows).
async fn sigterm() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        signal(SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    }
    #[cfg(not(unix))]
    {
        std::future::pending::<()>().await;
    }
}
