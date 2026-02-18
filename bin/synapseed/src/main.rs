#![forbid(unsafe_code)]
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use clap::{Parser, Subcommand};
use serde_json::json;
use tracing::{debug, error, info, warn};

use std::collections::HashSet;
use std::sync::Arc;

use parking_lot::Mutex;

use synapseed_core::context::SynapseContext;
use synapseed_core::event::{FileChangeKind, SynapseEvent};
use synapseed_core::liquid::ProjectDna;
use synapseed_core::momentum::{ModelTier, MomentumEngine};
use synapseed_core::plugin::SynapsePlugin;
use synapseed_core::pulse::PulseStore;
use synapseed_core::recorder::FlightRecorder;
use synapseed_core::state::ProjectState;
use synapseed_core::telemetry;

use synapseed_architect::plugin::ArchitectPlugin;
use synapseed_chronos::plugin::ChronosPlugin;
use synapseed_cortex::graph::CodeGraph;
use synapseed_cortex::plugin::CortexPlugin;
use synapseed_gym::plugin::GymPlugin;
use synapseed_husk::plugin::HuskPlugin;
use synapseed_janitor::plugin::JanitorPlugin;
use synapseed_root::plugin::RootPlugin;
use synapseed_search::plugin::SearchPlugin;
use synapseed_shadow_check::plugin::ShadowCheckPlugin;
use synapseed_telemetry_sink::plugin::TelemetrySinkPlugin;
use synapseed_whisper::plugin::WhisperPlugin;

use synapseed_mcp::protocol::ContentBlock;
use synapseed_mcp::tools::handle_tool_call;

// mod handlers; (removed)

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

    /// Output in JSON format
    #[arg(short, long, global = true)]
    json: bool,
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
        /// End line (default: 50, same as MCP)
        #[arg(short, long, default_value = "50")]
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

    /// Check if a file exists in the project (truth verification)
    #[command(visible_alias = "verify_path")]
    Verify {
        /// File path relative to project root
        path: String,
    },

    /// Analyze a compiled binary (ELF/Mach-O/PE)
    #[command(visible_alias = "decompile", visible_alias = "neural_decompiler")]
    AnalyzeBinary {
        /// Path to the binary file
        path: String,
    },

    /// Explain what a compiled Rust dependency does
    ExplainDependency {
        /// Name of the dependency crate
        crate_name: String,
    },

    /// Run a reproducible benchmark suite
    #[command(visible_alias = "bench", visible_alias = "benchmark")]
    RunBenchmark {
        /// Path to JSONL question suite
        suite_path: String,
        /// Output format: summary or json
        #[arg(short, long, default_value = "summary")]
        format: String,
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
            cmd_mcp(&project_root, "hoist", args, cli.json).await?
        }
        Commands::Lookup { name } => {
            cmd_mcp(&project_root, "lookup", json!({"name": name}), cli.json).await?
        }
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
            cmd_mcp(
                &project_root,
                "scan",
                json!({"content": text, "mode": mode}),
                cli.json,
            )
            .await?
        }
        Commands::Check { command } => {
            cmd_mcp(
                &project_root,
                "check",
                json!({"command": command}),
                cli.json,
            )
            .await?
        }
        Commands::Diagnose => cmd_mcp(&project_root, "diagnose", json!({}), cli.json).await?,
        Commands::History { limit } => cmd_history(&project_root, limit)?,
        Commands::Blame { file, start, end } => {
            cmd_mcp(
                &project_root,
                "blame",
                json!({"file": file, "start_line": start, "end_line": end}),
                cli.json,
            )
            .await?
        }
        Commands::Status => cmd_status(&project_root).await?,
        Commands::Init => cmd_init(&project_root).await?,
        Commands::Serve => cmd_serve(&project_root).await?,

        // ── MCP-bridged commands ────────────────────────────────────
        Commands::Ask { query, raw } => cmd_ask(&project_root, &query, raw, cli.json).await?,
        Commands::Search { query, limit } => {
            cmd_mcp(
                &project_root,
                "search",
                json!({"query": query, "limit": limit}),
                cli.json,
            )
            .await?
        }
        Commands::Diagnostics { file, min_severity } => {
            let mut args = json!({"min_severity": min_severity});
            if let Some(f) = file {
                args["file"] = json!(f);
            }
            cmd_mcp(&project_root, "diagnostics", args, cli.json).await?
        }
        Commands::Analyze { file, start, end } => {
            let mut args = json!({"file": file});
            if let Some(s) = start {
                args["start_line"] = json!(s);
            }
            if let Some(e) = end {
                args["end_line"] = json!(e);
            }
            cmd_mcp(&project_root, "analyze", args, cli.json).await?
        }
        Commands::Quickfix { file, error_code } => {
            cmd_mcp(
                &project_root,
                "quickfix",
                json!({"file": file, "error_code": error_code}),
                cli.json,
            )
            .await?
        }
        Commands::Intent { limit } => {
            cmd_mcp(&project_root, "intent", json!({"limit": limit}), cli.json).await?
        }
        Commands::Train {
            source,
            tests,
            timeout,
            fuzz,
            adversarial,
        } => {
            let src = read_source_or_stdin(&source)?;
            let mut args = json!({"source": src, "timeout": timeout, "fuzz": fuzz, "adversarial": adversarial});
            if let Some(t) = tests {
                args["tests"] = json!(read_source_or_stdin(&t)?);
            }
            cmd_mcp(&project_root, "train", args, cli.json).await?
        }
        Commands::ResetTelemetry => {
            cmd_mcp(&project_root, "reset-telemetry", json!({}), cli.json).await?
        }
        Commands::Janitor => cmd_mcp(&project_root, "janitor", json!({}), cli.json).await?,
        Commands::JanitorFix {
            proposal_id,
            confirm,
        } => {
            cmd_mcp(
                &project_root,
                "janitor-fix",
                json!({"proposal_id": proposal_id, "confirm": confirm}),
                cli.json,
            )
            .await?
        }
        Commands::Architect { refresh } => {
            cmd_mcp(
                &project_root,
                "architect",
                json!({"refresh": refresh}),
                cli.json,
            )
            .await?
        }
        Commands::Consult { query } => {
            cmd_mcp(&project_root, "consult", json!({"query": query}), cli.json).await?
        }
        Commands::Oracle => cmd_mcp(&project_root, "oracle", json!({}), cli.json).await?,
        Commands::Similar {
            query,
            top_k,
            min_similarity,
        } => {
            cmd_mcp(
                &project_root,
                "similar",
                json!({"query": query, "top_k": top_k, "min_similarity": min_similarity}),
                cli.json,
            )
            .await?
        }
        Commands::Verify { path } => {
            cmd_mcp(
                &project_root,
                "verify_path",
                json!({"path": path}),
                cli.json,
            )
            .await?
        }
        Commands::AnalyzeBinary { path } => {
            cmd_mcp(
                &project_root,
                "analyze_binary",
                json!({"path": path}),
                cli.json,
            )
            .await?
        }
        Commands::ExplainDependency { crate_name } => {
            cmd_mcp(
                &project_root,
                "explain_dependency",
                json!({"crate_name": crate_name}),
                cli.json,
            )
            .await?
        }
        Commands::RunBenchmark { suite_path, format } => {
            cmd_mcp(
                &project_root,
                "run_benchmark",
                json!({"suite_path": suite_path, "format": format}),
                cli.json,
            )
            .await?
        }
        Commands::External(args) => {
            let query = args.join(" ");
            cmd_ask(&project_root, &query, false, cli.json).await?
        }
    }

    Ok(())
}

// ── Shared helpers ──────────────────────────────────────────────────

/// Build the canonical plugin list — the SINGLE SOURCE OF TRUTH.
///
/// Both CLI and MCP serve mode MUST use this same list.  Adding a new
/// plugin?  Do it HERE and only here.
fn build_plugins(dna: &ProjectDna) -> Vec<Box<dyn SynapsePlugin>> {
    let mut plugins: Vec<Box<dyn SynapsePlugin>> = vec![
        Box::new(HuskPlugin::from_dna(dna)),
        Box::new(RootPlugin::new()),
        Box::new(CortexPlugin::new()),
        Box::new(ChronosPlugin::new()),
        Box::new(ShadowCheckPlugin::new()),
        Box::new(SearchPlugin::new()),
        Box::new(TelemetrySinkPlugin::new()),
        Box::new(ArchitectPlugin::new()),
        Box::new(WhisperPlugin::new()),
        Box::new(GymPlugin::new()),
        Box::new(JanitorPlugin::new()),
    ];
    plugins.sort_by_key(|p| p.priority());
    plugins
}

/// Resolve ModelTier: env SYNAPSEED_MODEL_TIER > DNA hci.model_profile > default
fn resolve_model_tier(dna: &ProjectDna) -> ModelTier {
    if let Ok(env_tier) = std::env::var("SYNAPSEED_MODEL_TIER") {
        let t = ModelTier::from_config(&env_tier).unwrap_or_default();
        info!(tier = %t, source = "env", "Model tier from SYNAPSEED_MODEL_TIER");
        t
    } else if let Some(profile) = &dna.hci.model_profile {
        let t = ModelTier::from_config(profile).unwrap_or_default();
        info!(tier = %t, source = "dna", "Model tier from DNA override");
        t
    } else {
        ModelTier::default()
    }
}

/// Build a fully-initialized SynapseContext — IDENTICAL to MCP serve mode.
///
/// This is the single entry point for all CLI commands.  It ensures:
/// - Full plugin list (same as serve)
/// - MomentumEngine, FlightRecorder, PulseStore registered
/// - Background indexing has time to complete
async fn init_full_context(path: &Path) -> Result<SynapseContext> {
    let state = ProjectState::detect(path);
    let dna = ProjectDna::load(path);
    let ctx = SynapseContext::new(path.to_path_buf(), state, dna.clone());

    // Register subsystems that MCP server registers in handle_initialize()
    let tier = resolve_model_tier(&dna);
    ctx.set_extension(Arc::new(Mutex::new(MomentumEngine::new(tier))));
    ctx.set_extension(Arc::new(Mutex::new(FlightRecorder::new())));
    ctx.set_extension(Arc::new(PulseStore::new()));

    let mut plugins = build_plugins(&dna);

    for plugin in &mut plugins {
        if let Err(e) = plugin.on_init(&ctx) {
            eprintln!("[WARN] {} failed to init: {e}", plugin.name());
        }
    }

    // Wait for background indexing — avoids race conditions for search/hoist/etc.
    wait_for_index(&ctx, Duration::from_secs(5)).await;

    Ok(ctx)
}

/// `ask` command — thin wrapper, context is now fully initialized by init_full_context().
async fn cmd_ask(path: &Path, query: &str, raw: bool, json_output: bool) -> Result<()> {
    let ctx = init_full_context(path).await?;

    let result = handle_tool_call("ask", &json!({"query": query, "raw": raw}), &ctx);

    if json_output {
        println!("{}", serde_json::to_string(&result)?);
    } else {
        for block in &result.content {
            match block {
                ContentBlock::Text { text } => println!("{text}"),
            }
        }
    }

    if result.is_error == Some(true) {
        std::process::exit(1);
    }

    Ok(())
}

/// Wait for background indexing to complete, with a timeout fallback.
///
/// Subscribes to the event bus and waits for both `IndexingComplete` (CodeGraph)
/// and `SearchReady` (Tantivy).  If the timeout expires before both fire,
/// ensures the CodeGraph is populated so the Whisperer's fallback pass works.
async fn wait_for_index(ctx: &SynapseContext, timeout: Duration) {
    let start = std::time::Instant::now();

    // Fast path: graph already populated (MCP serve mode, or index was instant)
    if let Some(graph) = ctx.get_extension::<CodeGraph>() {
        if graph.file_count() > 0 {
            // CodeGraph ready — wait for Tantivy with remaining timeout budget.
            // Large repos (Django: 55K symbols) need >500ms; use whatever time
            // remains from the outer timeout, with a minimum of 500ms.
            let elapsed = start.elapsed();
            let remaining = timeout.saturating_sub(elapsed);
            let tantivy_wait = remaining.max(Duration::from_millis(500));
            debug!(
                symbols = graph.symbol_count(),
                wait_ms = tantivy_wait.as_millis() as u64,
                "wait_for_index: CodeGraph ready, waiting for Tantivy"
            );

            let mut rx = ctx.subscribe();
            match tokio::time::timeout(tantivy_wait, async {
                loop {
                    match rx.recv().await {
                        Ok(SynapseEvent::SearchReady) => break,
                        Err(_) => break,
                        _ => continue,
                    }
                }
            })
            .await
            {
                Ok(()) => debug!("wait_for_index: SearchReady received"),
                Err(_) => debug!(
                    wait_ms = tantivy_wait.as_millis() as u64,
                    "wait_for_index: Tantivy timeout elapsed, proceeding without SearchReady"
                ),
            }
            return;
        }
    }

    // Subscribe and wait for both IndexingComplete + SearchReady
    let mut rx = ctx.subscribe();
    let mut got_index = false;
    let mut got_search = false;
    let result = tokio::time::timeout(timeout, async {
        loop {
            match rx.recv().await {
                Ok(SynapseEvent::IndexingComplete) => {
                    got_index = true;
                    if got_search {
                        break;
                    }
                }
                Ok(SynapseEvent::SearchReady) => {
                    got_search = true;
                    if got_index {
                        break;
                    }
                }
                Err(_) => break, // channel closed
                _ => continue,
            }
        }
    })
    .await;

    if result.is_err() {
        // Timeout — ensure CodeGraph is populated for Whisperer fallback
        if !got_index {
            warn!("Index timeout ({timeout:?}), performing synchronous hoist");
            if let Some(graph) = ctx.get_extension::<CodeGraph>() {
                if graph.file_count() == 0 {
                    let root = ctx.project_root();
                    if let Err(e) = graph.index_directory(&root) {
                        warn!(error = %e, "Synchronous hoist failed");
                    } else {
                        info!(
                            files = graph.file_count(),
                            symbols = graph.symbol_count(),
                            "Synchronous hoist completed"
                        );
                    }
                }
            }
        }
        if !got_search {
            debug!("Tantivy not ready — Whisperer will use CodeGraph fallback");
        }
    }
}

/// Generic MCP tool bridge: init context, call tool, print result.
async fn cmd_mcp(
    path: &Path,
    tool: &str,
    args: serde_json::Value,
    json_output: bool,
) -> Result<()> {
    let ctx = init_full_context(path).await?;

    // When the CLI requests JSON output, inject a hint so tools can return
    // structured data instead of human-readable text.
    let mut args = args;
    if json_output {
        if let Some(obj) = args.as_object_mut() {
            obj.insert(
                "_format".to_string(),
                serde_json::Value::String("json".to_string()),
            );
        }
    }

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

async fn cmd_status(path: &Path) -> Result<()> {
    let state = ProjectState::detect(path);
    let dna = ProjectDna::load(path);
    let ctx = SynapseContext::new(path.to_path_buf(), state.clone(), dna.clone());

    // Use the SAME plugin list as serve mode
    let mut plugins = build_plugins(&dna);

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
    let ctx = SynapseContext::new(path.to_path_buf(), state.clone(), dna.clone());

    println!("=== SYNAPSEED INIT ===\n");
    println!("{}\n", state.diagnostic());

    // Use the SAME plugin list as serve mode
    let mut plugins = build_plugins(&dna);

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

    // Use the SAME plugin list as all other modes
    let mut plugins = build_plugins(&dna);

    for plugin in &mut plugins {
        if let Err(e) = plugin.on_init(&ctx) {
            eprintln!("[WARN] {} failed to init: {e}", plugin.name());
        }
    }

    info!("Starting MCP server on stdio");

    // ── Spawn FileWatcher (Intervento 1: Reactive Event Bus) ──────
    let watcher_handle = spawn_file_watcher(path, &ctx);

    // ── Spawn Plugin Dispatch Loop ────────────────────────────────
    // Moves plugins into an Arc for shared ownership between the
    // dispatch loop and the shutdown sequence.
    let plugins = Arc::new(plugins);
    spawn_plugin_dispatch_loop(plugins.clone(), &ctx);

    // ── Spawn RepairOrchestrator (Intervento 5: Action Chaining) ──
    // Register NotificationSink before the orchestrator needs it.
    let sink = synapseed_mcp::notification_sink::spawn_notification_sink();
    ctx.set_extension(Arc::new(sink));
    synapseed_mcp::repair::spawn_repair_orchestrator(&ctx);

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

    // 4. Stop the file watcher
    drop(watcher_handle);

    // 5. Brief grace period for background tasks to finish
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    eprintln!("[INFO] Shutdown complete");
    Ok(())
}

// ── FileWatcher (Intervento 1: Primum Movens) ────────────────────────
//
// Watches the project directory for file changes and emits `FileChanged`
// events on the bus, awakening the dormant reactive plugin chain:
//   Husk(P10) → Cortex(P50) → ShadowCheck(P150) → Architect(P150) → Search(P200)
//
// Uses `notify-debouncer-mini` for 500ms batching (handles IDE multi-save).
// Adaptive throttling: if burst > 50 files, cooldown doubles (max 5s).

/// Opaque handle that keeps the watcher alive; dropping it stops watching.
struct WatcherHandle {
    _watcher: notify_debouncer_mini::Debouncer<notify_debouncer_mini::notify::RecommendedWatcher>,
}

/// Directories and patterns to ignore.
const IGNORED_COMPONENTS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    ".synapseed",
    "__pycache__",
    ".DS_Store",
    ".mypy_cache",
    ".pytest_cache",
];

/// File extensions that are considered source files.
fn is_source_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| {
            matches!(
                ext,
                "rs" | "toml"
                    | "py"
                    | "js"
                    | "ts"
                    | "jsx"
                    | "tsx"
                    | "json"
                    | "yaml"
                    | "yml"
                    | "md"
                    | "txt"
            )
        })
        .unwrap_or(false)
}

/// Check whether a path should be ignored.
fn should_ignore(path: &Path) -> bool {
    path.components().any(|c| {
        c.as_os_str()
            .to_str()
            .map(|s| IGNORED_COMPONENTS.contains(&s))
            .unwrap_or(false)
    })
}

fn spawn_file_watcher(project_root: &Path, ctx: &SynapseContext) -> WatcherHandle {
    use notify_debouncer_mini::new_debouncer;
    use notify_debouncer_mini::notify::RecursiveMode;

    let ctx = ctx.clone();
    let root = project_root.to_path_buf();

    // The debouncer batches events into 500ms windows.
    // The callback fires on a background thread from `notify`.
    let debouncer = new_debouncer(
        Duration::from_millis(500),
        move |result: Result<
            Vec<notify_debouncer_mini::DebouncedEvent>,
            notify_debouncer_mini::notify::Error,
        >| {
            let events = match result {
                Ok(evts) => evts,
                Err(e) => {
                    warn!(error = %e, "FileWatcher: notify error");
                    return;
                }
            };

            // Dedup by path — a file saved 10 times in 500ms → 1 event
            let mut seen = HashSet::new();
            let mut batch: Vec<(PathBuf, FileChangeKind)> = Vec::new();

            for evt in &events {
                let path = &evt.path;
                if should_ignore(path) || !is_source_file(path) {
                    continue;
                }
                if !seen.insert(path.clone()) {
                    continue; // already in this batch
                }

                let kind = if !path.exists() {
                    FileChangeKind::Deleted
                } else {
                    // notify-debouncer-mini doesn't distinguish create vs modify
                    FileChangeKind::Modified
                };
                batch.push((path.clone(), kind));
            }

            if batch.is_empty() {
                return;
            }

            // Adaptive throttle: if huge batch, log a warning
            if batch.len() > 50 {
                warn!(
                    count = batch.len(),
                    "FileWatcher: large batch detected (mass refactoring?), processing all"
                );
            }

            debug!(
                count = batch.len(),
                "FileWatcher: emitting FileChanged events"
            );

            for (path, kind) in batch {
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                ctx.broadcast(SynapseEvent::FileChanged { path: rel, kind });
            }
        },
    )
    .expect("FileWatcher: failed to create debouncer");

    // Start watching the project root recursively
    // Note: we use the inner watcher via `debouncer.watcher()`
    // Actually, notify-debouncer-mini's Debouncer implements Deref to watcher
    // But the API requires adding the watch path after creation.
    // Let's use the watcher method.
    let mut debouncer = debouncer;
    if let Err(e) = debouncer
        .watcher()
        .watch(project_root, RecursiveMode::Recursive)
    {
        warn!(error = %e, "FileWatcher: failed to watch project root, continuing without file watching");
    } else {
        info!(path = %project_root.display(), "FileWatcher: watching for changes");
    }

    WatcherHandle {
        _watcher: debouncer,
    }
}

// ── Plugin Dispatch Loop ─────────────────────────────────────────────
//
// Subscribes to the event bus and dispatches events to all plugins in
// priority order.  This is the missing piece that connects FileWatcher
// events to plugin `on_event()` handlers in MCP serve mode.
//
// Reentrancy guard: max depth of 3 to prevent infinite event cascading.

/// Maximum event chain depth before dropping events.
const MAX_EVENT_DEPTH: u8 = 3;

fn spawn_plugin_dispatch_loop(plugins: Arc<Vec<Box<dyn SynapsePlugin>>>, ctx: &SynapseContext) {
    let mut rx = ctx.subscribe();
    let ctx = ctx.clone();

    tokio::spawn(async move {
        // Track event chain depth to prevent infinite loops
        let depth = Arc::new(std::sync::atomic::AtomicU8::new(0));

        loop {
            match rx.recv().await {
                Ok(event) => {
                    // Skip lifecycle events handled elsewhere
                    if matches!(event, SynapseEvent::SystemInit { .. }) {
                        continue;
                    }
                    if matches!(event, SynapseEvent::SystemShutdown) {
                        debug!("PluginDispatchLoop: received SystemShutdown, exiting");
                        break;
                    }

                    // Reentrancy guard
                    let current_depth = depth.load(std::sync::atomic::Ordering::Relaxed);
                    if current_depth >= MAX_EVENT_DEPTH {
                        debug!(
                            depth = current_depth,
                            event = ?std::mem::discriminant(&event),
                            "PluginDispatchLoop: max depth exceeded, dropping event"
                        );
                        continue;
                    }

                    let plugins = plugins.clone();
                    let ctx = ctx.clone();
                    let depth = depth.clone();

                    // Dispatch on a blocking thread (plugin handlers may do I/O)
                    tokio::task::spawn_blocking(move || {
                        // D60: Skip processing if shutdown was requested while queued
                        if ctx.is_shutting_down() {
                            return;
                        }

                        depth.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                        let rt = tokio::runtime::Handle::current();
                        for plugin in plugins.iter() {
                            // D44: catch_unwind prevents a panicking plugin from
                            // killing the entire dispatch loop.
                            let result =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    rt.block_on(plugin.on_event(&event, &ctx))
                                }));
                            match result {
                                Ok(Ok(Some(new_event))) => {
                                    debug!(
                                        plugin = plugin.name(),
                                        event = ?std::mem::discriminant(&new_event),
                                        "PluginDispatchLoop: plugin emitted follow-up event"
                                    );
                                    ctx.broadcast(new_event);
                                }
                                Ok(Ok(None)) => {}
                                Ok(Err(e)) => {
                                    warn!(
                                        plugin = plugin.name(),
                                        error = %e,
                                        "PluginDispatchLoop: plugin event error"
                                    );
                                }
                                Err(_) => {
                                    error!(
                                        plugin = plugin.name(),
                                        "PluginDispatchLoop: plugin panicked, continuing"
                                    );
                                }
                            }
                        }

                        depth.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                    })
                    .await
                    .ok();
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    debug!(skipped = n, "PluginDispatchLoop: lagged, dropped events");
                }
                Err(_) => break, // Channel closed
            }
        }
        debug!("PluginDispatchLoop: exiting");
    });
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
