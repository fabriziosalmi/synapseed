//! Telemetry Sink Plugin — spawns the OTLP gRPC receiver.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tracing::{info, warn};

use synapseed_core::context::SynapseContext;
use synapseed_core::error::Result;
use synapseed_core::event::SynapseEvent;
use synapseed_core::plugin::SynapsePlugin;

use crate::server;
use crate::store::SpanStore;

/// The Telemetry Sink plugin — OTLP gRPC receiver.
pub struct TelemetrySinkPlugin {
    port: u16,
}

impl TelemetrySinkPlugin {
    pub fn new() -> Self {
        Self { port: 4317 }
    }

    pub fn with_port(port: u16) -> Self {
        Self { port }
    }
}

impl Default for TelemetrySinkPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl SynapsePlugin for TelemetrySinkPlugin {
    fn name(&self) -> &str {
        "telemetry-sink"
    }

    fn on_init(&mut self, ctx: &SynapseContext) -> Result<()> {
        let store = SpanStore::new();
        let port = self.port;

        // Register store as a context extension for MCP tools/resources
        ctx.set_extension(Arc::new(store.clone()));

        // Spawn the gRPC server
        let ctx_for_server = ctx.clone();
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        tokio::spawn(async move {
            if let Err(e) = server::start(addr, store, ctx_for_server).await {
                warn!(error = %e, "Telemetry: gRPC server failed to start");
            }
        });

        info!(port = port, "Telemetry: OTLP receiver at 127.0.0.1:{port}");
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
        200 // Before visualizer (250), after core subsystems
    }
}
