//! Axum HTTP/WebSocket server for the architecture dashboard.

use std::net::SocketAddr;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use rust_embed::Embed;
use serde_json::json;
use tracing::{info, warn};

use synapseed_core::context::SynapseContext;
use synapseed_cortex::graph::CodeGraph;
use synapseed_telemetry_sink::store::SpanStore;

// ── Embedded Assets ──────────────────────────────────────────────

#[derive(Embed)]
#[folder = "assets/"]
struct Assets;

// ── App State ────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    ctx: SynapseContext,
}

// ── Server Entry Point ───────────────────────────────────────────

/// Start the axum server. Blocks until the server shuts down.
pub async fn start(addr: SocketAddr, ctx: SynapseContext) -> anyhow::Result<()> {
    let state = AppState { ctx };

    let app = Router::new()
        .route("/", get(serve_index))
        .route("/graph.js", get(serve_graph_js))
        .route("/api/graph", get(api_graph))
        .route("/ws", get(ws_upgrade))
        .with_state(state);

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            warn!(addr = %addr, error = %e, "Visualizer: Failed to bind port");
            return Err(e.into());
        }
    };

    info!(addr = %addr, "Visualizer: Server listening");
    axum::serve(listener, app).await?;
    Ok(())
}

// ── Static Asset Handlers ────────────────────────────────────────

async fn serve_index() -> impl IntoResponse {
    serve_embedded("index.html", "text/html; charset=utf-8")
}

async fn serve_graph_js() -> impl IntoResponse {
    serve_embedded("graph.js", "application/javascript; charset=utf-8")
}

fn serve_embedded(path: &str, content_type: &str) -> impl IntoResponse {
    match Assets::get(path) {
        Some(file) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, content_type.to_string())],
            file.data.to_vec(),
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}

// ── Graph API ────────────────────────────────────────────────────

async fn api_graph(State(state): State<AppState>) -> impl IntoResponse {
    let ctx = state.ctx;
    let root = ctx.project_root();

    // Collect heatmap data from telemetry store (if available)
    let hotspot_map: std::collections::HashMap<String, f64> = ctx
        .get_extension::<SpanStore>()
        .map(|store| {
            store
                .hotspots()
                .into_iter()
                .map(|m| (m.key.clone(), m.avg_duration_ms))
                .collect()
        })
        .unwrap_or_default();

    // Try the shared CodeGraph from CortexPlugin (already indexed at startup).
    // Falls back to building an ephemeral graph if no shared graph is available.
    let shared_graph = ctx.get_extension::<CodeGraph>();

    let graph_result = tokio::task::spawn_blocking(move || {
        if let Some(ref graph) = shared_graph {
            if graph.file_count() > 0 {
                return Ok(build_cytoscape_data(graph, &hotspot_map));
            }
        }
        // Fallback: build ephemeral graph
        let graph = CodeGraph::new();
        if let Err(e) = graph.index_directory(&root) {
            return Err(format!("Index error: {e}"));
        }
        Ok(build_cytoscape_data(&graph, &hotspot_map))
    })
    .await;

    match graph_result {
        Ok(Ok(data)) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json".to_string())],
            serde_json::to_string(&data).unwrap_or_default(),
        )
            .into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Task failed: {e}"),
        )
            .into_response(),
    }
}

/// Convert the CodeGraph into Cytoscape.js elements format.
/// `hotspot_map` maps "file:symbol" keys to average duration in ms.
fn build_cytoscape_data(
    graph: &CodeGraph,
    hotspot_map: &std::collections::HashMap<String, f64>,
) -> serde_json::Value {
    let mut nodes = Vec::new();

    for file in graph.all_files() {
        // Shorten the file path for display
        let label = file
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&file.path)
            .to_string();

        let file_id = format!("file:{}", file.path);

        // Check if any symbol in this file is a hotspot
        let file_heat = hotspot_map
            .iter()
            .filter(|(k, _)| k.starts_with(&file.path))
            .map(|(_, v)| *v)
            .fold(0.0f64, f64::max);
        let file_heat_level = heat_level(file_heat);

        // File node (compound parent)
        nodes.push(json!({
            "data": {
                "id": file_id,
                "label": label,
                "type": "file",
                "language": file.language,
                "fullPath": file.path,
                "heatLevel": file_heat_level,
                "heatMs": file_heat,
            }
        }));

        // Symbol nodes (children of file via "parent" — Cytoscape compound nodes)
        for sym in &file.symbols {
            let sym_id = format!("sym:{}:{}", file.path, sym.name);
            let sym_label = match &sym.signature {
                Some(sig) => {
                    if sig.len() > 40 {
                        format!("{}...", &sig[..37])
                    } else {
                        sig.clone()
                    }
                }
                None => sym.name.clone(),
            };

            // Check hotspot for this specific symbol
            let key = format!("{}:{}", file.path, sym.name);
            let sym_heat = hotspot_map.get(&key).copied().unwrap_or(0.0);
            let sym_heat_level = heat_level(sym_heat);

            nodes.push(json!({
                "data": {
                    "id": sym_id,
                    "label": sym_label,
                    "type": format!("{:?}", sym.kind).to_lowercase(),
                    "parent": file_id,
                    "name": sym.name,
                    "kind": format!("{:?}", sym.kind),
                    "lineStart": sym.line_start,
                    "lineEnd": sym.line_end,
                    "heatLevel": sym_heat_level,
                    "heatMs": sym_heat,
                }
            }));
        }
    }

    json!({
        "elements": {
            "nodes": nodes,
        },
        "stats": {
            "files": graph.file_count(),
            "symbols": graph.symbol_count(),
        }
    })
}

/// Classify a duration into a heat level for the visualizer.
fn heat_level(avg_ms: f64) -> &'static str {
    if avg_ms > 200.0 {
        "hot"
    } else if avg_ms > 50.0 {
        "warm"
    } else if avg_ms > 0.0 {
        "cool"
    } else {
        "none"
    }
}

// ── WebSocket Handler ────────────────────────────────────────────

async fn ws_upgrade(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: AppState) {
    info!("Visualizer: WebSocket client connected");

    let mut rx = state.ctx.subscribe();

    loop {
        match rx.recv().await {
            Ok(event) => {
                let json = serde_json::to_string(&event).unwrap_or_default();
                if socket.send(Message::Text(json)).await.is_err() {
                    break; // Client disconnected
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                warn!(missed = n, "Visualizer: WebSocket client lagging");
                // Send a lag notification
                let msg = json!({"type": "lag", "missed": n}).to_string();
                if socket.send(Message::Text(msg)).await.is_err() {
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                break; // Channel closed
            }
        }
    }

    info!("Visualizer: WebSocket client disconnected");
}
