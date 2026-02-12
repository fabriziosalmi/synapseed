use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Parser, Subcommand};
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
use synapseed_husk::guard::SecurityGuard;
use synapseed_husk::plugin::HuskPlugin;
use synapseed_root::plugin::RootPlugin;
use synapseed_root::sentinel::Sentinel;
use synapseed_search::plugin::SearchPlugin;
use synapseed_shadow_check::plugin::ShadowCheckPlugin;
use synapseed_telemetry_sink::plugin::TelemetrySinkPlugin;
use synapseed_visualizer::plugin::VisualizerPlugin;
use synapseed_gym::plugin::GymPlugin;
use synapseed_janitor::plugin::JanitorPlugin;
use synapseed_whisper::plugin::WhisperPlugin;

#[derive(Parser)]
#[command(
    name = "synapseed",
    about = "High-Performance Semantic AI Middleware — The Thinking Layer Between You and the LLM",
    version,
    after_help = "SYNAPSEED: Where intelligence meets infrastructure."
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
    /// Index a project and display its code graph skeleton
    Hoist,

    /// Look up a symbol by name across the indexed project
    Lookup {
        /// Symbol name to search for
        name: String,
    },

    /// Scan content for sensitive data (DLP check)
    Scan {
        /// Text to scan (reads from stdin if not provided)
        #[arg(short, long)]
        text: Option<String>,
    },

    /// Evaluate a command against the security sentinel
    Check {
        /// Command to evaluate
        command: String,
    },

    /// Run full system diagnostic — detect state, load plugins, report
    Diagnose,

    /// Show git history summary and recent commits
    History {
        /// Number of recent commits to show
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },

    /// Show blame information for a file
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
        Commands::Hoist => cmd_hoist(&project_root)?,
        Commands::Lookup { name } => cmd_lookup(&name, &project_root)?,
        Commands::Scan { text } => cmd_scan(text.as_deref())?,
        Commands::Check { command } => cmd_check(&command)?,
        Commands::Diagnose => cmd_diagnose(&project_root)?,
        Commands::History { limit } => cmd_history(&project_root, limit)?,
        Commands::Blame { file, start, end } => cmd_blame(&project_root, &file, start, end)?,
        Commands::Status => cmd_status(&project_root).await?,
        Commands::Init => cmd_init(&project_root).await?,
        Commands::Serve => cmd_serve(&project_root).await?,
    }

    Ok(())
}

fn cmd_hoist(path: &Path) -> Result<()> {
    let graph = CodeGraph::new();

    info!(path = %path.display(), "Indexing project");
    graph.index_directory(path)?;

    println!(
        "Indexed {} files, {} symbols\n",
        graph.file_count(),
        graph.symbol_count()
    );

    let json = serde_json::to_string_pretty(&serde_json::json!({
        "project": "synapseed",
        "files_indexed": graph.file_count(),
        "symbols_indexed": graph.symbol_count(),
    }))?;
    println!("{json}");

    Ok(())
}

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

fn cmd_scan(text: Option<&str>) -> Result<()> {
    let content = match text {
        Some(t) => t.to_string(),
        None => {
            use std::io::Read;
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            buf
        }
    };

    let guard = SecurityGuard::with_defaults();

    match guard.check(&content) {
        Ok(()) => println!("CLEAN: No sensitive data detected."),
        Err(e) => {
            println!("ALERT: {e}");
            let sanitized = guard.redact(&content);
            println!("\nSanitized output:\n{sanitized}");
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

    // Run MCP server with graceful shutdown on SIGINT/SIGTERM
    tokio::select! {
        result = synapseed_mcp::server::run(ctx) => {
            result?;
        }
        _ = tokio::signal::ctrl_c() => {
            eprintln!("[INFO] Received shutdown signal, exiting gracefully...");
        }
    }

    Ok(())
}
