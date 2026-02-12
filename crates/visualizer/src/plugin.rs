//! Visualizer Plugin — spawns the dashboard server and file watcher.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;

use notify::{RecursiveMode, Watcher};
use tracing::{info, warn};

use synapseed_core::context::SynapseContext;
use synapseed_core::error::Result;
use synapseed_core::event::{FileChangeKind, SynapseEvent};
use synapseed_core::plugin::SynapsePlugin;

use crate::server;

/// The Visualizer plugin — live architecture dashboard.
pub struct VisualizerPlugin {
    port: u16,
}

impl VisualizerPlugin {
    pub fn new() -> Self {
        Self { port: 3000 }
    }

    pub fn with_port(port: u16) -> Self {
        Self { port }
    }

    /// Create from DNA config and environment variables.
    /// Priority: SYNAPSEED_VISUALIZER_PORT env > dna.visualizer_port > 3000
    pub fn from_config(dna: &synapseed_core::liquid::ProjectDna) -> Self {
        let port = std::env::var("SYNAPSEED_VISUALIZER_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .or(dna.visualizer_port)
            .unwrap_or(3000);
        Self { port }
    }
}

impl Default for VisualizerPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl SynapsePlugin for VisualizerPlugin {
    fn name(&self) -> &str {
        "visualizer"
    }

    fn on_init(&mut self, ctx: &SynapseContext) -> Result<()> {
        let root = ctx.project_root();
        let port = self.port;
        let port_retry = ctx.dna().hci.port_retry;

        // 1. Spawn the HTTP/WS server on the tokio runtime
        let ctx_for_server = ctx.clone();
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        tokio::spawn(async move {
            if let Err(e) = server::start(addr, ctx_for_server, port_retry).await {
                warn!(error = %e, "Visualizer: Server failed to start");
            }
        });

        // 2. Start file watcher on a background thread
        start_file_watcher(&root, ctx.clone());

        info!(
            port = port,
            "Visualizer: Dashboard at http://localhost:{port}"
        );
        Ok(())
    }

    fn on_event<'a>(
        &'a self,
        _event: &'a SynapseEvent,
        _ctx: &'a SynapseContext,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SynapseEvent>>> + Send + 'a>> {
        Box::pin(async move { Ok(None) })
    }

    fn priority(&self) -> u32 {
        250 // Low priority — visualization is non-critical
    }
}

/// Start a file system watcher that bridges notify events into the SynapseEvent bus.
fn start_file_watcher(root: &Path, ctx: SynapseContext) {
    let root = root.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel::<notify::Event>();

    let watcher_result = notify::RecommendedWatcher::new(
        move |res: std::result::Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        },
        notify::Config::default(),
    );

    let mut watcher = match watcher_result {
        Ok(w) => w,
        Err(e) => {
            warn!(error = %e, "Visualizer: Failed to create file watcher");
            return;
        }
    };

    if let Err(e) = watcher.watch(&root, RecursiveMode::Recursive) {
        warn!(error = %e, "Visualizer: Failed to watch directory");
        return;
    }

    // Bridge thread: converts notify events → SynapseEvent broadcasts
    std::thread::spawn(move || {
        let _watcher = watcher; // prevent drop — keeps watching
        while let Ok(event) = rx.recv() {
            for path in &event.paths {
                let path_str = path.display().to_string();

                // Skip build artifacts, hidden dirs, and non-source files
                if path_str.contains("/target/")
                    || path_str.contains("/.git/")
                    || path_str.contains("/node_modules/")
                {
                    continue;
                }

                let kind = match event.kind {
                    notify::EventKind::Create(_) => FileChangeKind::Created,
                    notify::EventKind::Modify(_) => FileChangeKind::Modified,
                    notify::EventKind::Remove(_) => FileChangeKind::Deleted,
                    _ => continue,
                };

                ctx.broadcast(SynapseEvent::FileChanged {
                    path: path_str,
                    kind,
                });
            }
        }
    });

    info!("Visualizer: File watcher active");
}
