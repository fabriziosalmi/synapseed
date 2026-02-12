//! Axum HTTP/WebSocket server for the architecture dashboard.

use std::net::SocketAddr;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use rust_embed::Embed;
use serde_json::json;
use tracing::{debug, info, warn};

use synapseed_architect::ReportStore;
use synapseed_chronos::historian::Historian;
use synapseed_core::context::SynapseContext;
use synapseed_core::symbol::SymbolKind;
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

/// Maximum port retry attempts (HCI Req 1: Zero-Friction Start / port-hopping).
const PORT_RETRY_LIMIT: u16 = 10;

/// Bind a TCP listener, optionally retrying on the next port if occupied.
///
/// When `retry` is true, tries up to `PORT_RETRY_LIMIT` consecutive ports
/// starting from `base_port`. When false, fails immediately if the port is taken.
pub(crate) async fn bind_with_retry(
    host: std::net::IpAddr,
    base_port: u16,
    retry: bool,
) -> anyhow::Result<tokio::net::TcpListener> {
    if retry {
        let mut port = base_port;
        loop {
            let try_addr = SocketAddr::new(host, port);
            match tokio::net::TcpListener::bind(try_addr).await {
                Ok(l) => return Ok(l),
                Err(e) if port < base_port + PORT_RETRY_LIMIT => {
                    debug!(port, error = %e, "Visualizer: Port taken, trying next");
                    port += 1;
                }
                Err(e) => {
                    warn!(
                        base_port,
                        last_port = port,
                        error = %e,
                        "Visualizer: All port attempts exhausted"
                    );
                    return Err(e.into());
                }
            }
        }
    } else {
        let addr = SocketAddr::new(host, base_port);
        tokio::net::TcpListener::bind(addr).await.map_err(|e| {
            warn!(addr = %addr, error = %e, "Visualizer: Failed to bind port");
            e.into()
        })
    }
}

/// Start the axum server. Blocks until the server shuts down.
///
/// If `port_retry` is true, automatically tries the next port (up to 10 attempts)
/// when the requested port is already in use.
pub async fn start(
    addr: SocketAddr,
    ctx: SynapseContext,
    port_retry: bool,
) -> anyhow::Result<()> {
    let token = ctx.shutdown_token();
    let state = AppState { ctx };

    let app = Router::new()
        .route("/", get(serve_index))
        .route("/{name}.js", get(serve_js))
        .route("/api/graph", get(api_graph))
        .route("/api/xray", get(api_xray))
        .route("/ws", get(ws_upgrade))
        .with_state(state);

    let listener = bind_with_retry(addr.ip(), addr.port(), port_retry).await?;

    let actual_addr = listener.local_addr()?;
    info!(addr = %actual_addr, "Visualizer: Server listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async move { token.cancelled().await })
        .await?;
    info!("Visualizer: Server stopped");
    Ok(())
}

// ── Static Asset Handlers ────────────────────────────────────────

async fn serve_index() -> impl IntoResponse {
    serve_embedded("index.html", "text/html; charset=utf-8")
}

async fn serve_js(Path(name): Path<String>) -> impl IntoResponse {
    let filename = format!("{name}.js");
    serve_embedded(&filename, "application/javascript; charset=utf-8")
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

    // Get architect report for dependency edges (if available)
    let architect_report = ctx
        .get_extension::<ReportStore>()
        .and_then(|store| store.get());

    // Try the shared CodeGraph from CortexPlugin (already indexed at startup).
    // Falls back to building an ephemeral graph if no shared graph is available.
    let shared_graph = ctx.get_extension::<CodeGraph>();

    let graph_result = tokio::task::spawn_blocking(move || {
        // Build per-file rigidity map from git history (best-effort)
        let rigidity_map: std::collections::HashMap<String, f64> =
            if let Ok(historian) = Historian::open(&root) {
                let files: Vec<String> = shared_graph
                    .as_ref()
                    .map(|g| g.all_files().iter().map(|f| f.path.clone()).collect())
                    .unwrap_or_default();
                files
                    .iter()
                    .filter_map(|f| {
                        historian
                            .analyze_history(f, None, None)
                            .ok()
                            .map(|a| (f.clone(), a.rigidity))
                    })
                    .collect()
            } else {
                std::collections::HashMap::new()
            };

        if let Some(ref graph) = shared_graph {
            if graph.file_count() > 0 {
                return Ok(build_cytoscape_data(graph, &hotspot_map, architect_report.as_ref(), &rigidity_map));
            }
        }
        // Fallback: build ephemeral graph
        let graph = CodeGraph::new();
        if let Err(e) = graph.index_directory(&root) {
            return Err(format!("Index error: {e}"));
        }
        Ok(build_cytoscape_data(&graph, &hotspot_map, architect_report.as_ref(), &rigidity_map))
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
/// `architect_report` adds dependency edges and cycle annotations when available.
/// `rigidity_map` maps file paths to historical modification rigidity (0.0–1.0).
fn build_cytoscape_data(
    graph: &CodeGraph,
    hotspot_map: &std::collections::HashMap<String, f64>,
    architect_report: Option<&synapseed_architect::ArchitectureReport>,
    rigidity_map: &std::collections::HashMap<String, f64>,
) -> serde_json::Value {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // Collect cycle-involved modules for annotation
    let cycle_modules: std::collections::HashSet<String> = architect_report
        .map(|r| {
            r.violations
                .iter()
                .filter(|v| v.rule == "circular_dependency")
                .flat_map(|v| v.modules.iter().cloned())
                .collect()
        })
        .unwrap_or_default();

    // Build instability lookup from architect metrics
    let instability_map: std::collections::HashMap<String, f64> = architect_report
        .map(|r| {
            r.modules
                .iter()
                .map(|m| (m.module_name.clone(), m.instability))
                .collect()
        })
        .unwrap_or_default();

    // Build coupling lookup (efferent + afferent) for "Visual Physics"
    let coupling_map: std::collections::HashMap<String, (usize, usize)> = architect_report
        .map(|r| {
            r.modules
                .iter()
                .map(|m| {
                    (
                        m.module_name.clone(),
                        (m.efferent_coupling, m.afferent_coupling),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    // Topological density for the whole graph
    let topological_density: Option<f64> = architect_report.map(|r| r.topological_density);

    // Track unknown-language files for compound grouping (HCI Req 2: Chaos Tolerance)
    let mut unknown_file_count = 0usize;

    for file in graph.all_files() {
        // Shorten the file path for display
        let label = file
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&file.path)
            .to_string();

        let file_id = format!("file:{}", file.path);
        let is_unknown_lang = file.language == "unknown";

        // Check if any symbol in this file is a hotspot
        let file_heat = hotspot_map
            .iter()
            .filter(|(k, _)| k.starts_with(&file.path))
            .map(|(_, v)| *v)
            .fold(0.0f64, f64::max);
        let file_heat_level = heat_level(file_heat);

        // Check architect annotations
        let file_stem = file.path.rsplit('/').next().unwrap_or(&file.path);
        let file_stem_no_ext = file_stem.rsplit('.').next_back().unwrap_or(file_stem);
        let in_cycle = cycle_modules.iter().any(|m| m.ends_with(file_stem_no_ext));
        let instability = instability_map
            .iter()
            .find(|(k, _)| k.ends_with(file_stem_no_ext))
            .map(|(_, v)| *v);

        // Visual Physics: coupling and rigidity classification
        let coupling = coupling_map
            .iter()
            .find(|(k, _)| k.ends_with(file_stem_no_ext))
            .map(|(_, v)| *v);
        let total_coupling = coupling.map(|(ce, ca)| ce + ca).unwrap_or(0);
        let physics_class = physics_level(total_coupling, instability.unwrap_or(0.5));

        // Per-file rigidity from Chronos git history (0.0 = easy, 1.0 = rigid)
        let rigidity = rigidity_map.get(&file.path).copied().unwrap_or(0.0);

        // File node (compound parent)
        let mut file_data = json!({
            "id": file_id,
            "label": label,
            "type": "file",
            "language": file.language,
            "fullPath": file.path,
            "heatLevel": file_heat_level,
            "heatMs": file_heat,
            "inCycle": in_cycle,
            "instability": instability,
            "coupling": total_coupling,
            "physicsClass": physics_class,
            "rigidity": rigidity,
        });

        // Group unknown-language files under a "Unrecognized" compound node
        if is_unknown_lang {
            file_data["parent"] = json!("cluster:unrecognized");
            unknown_file_count += 1;
        }

        nodes.push(json!({ "data": file_data }));

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

    // Add compound parent for unknown-language files
    if unknown_file_count > 0 {
        nodes.insert(
            0,
            json!({
                "data": {
                    "id": "cluster:unrecognized",
                    "label": format!("Unrecognized ({unknown_file_count} files)"),
                    "type": "cluster",
                    "language": "unknown",
                }
            }),
        );
    }

    // Add dependency edges from architect report
    if let Some(report) = architect_report {
        // Build a mapping from module name suffix to file node ID
        let file_ids: Vec<String> = graph
            .all_files()
            .iter()
            .map(|f| f.path.clone())
            .collect();

        for module in &report.modules {
            // Find the file node for this module's file
            let source_file = file_ids
                .iter()
                .find(|f| f.as_str() == module.file_path || f.ends_with(&module.file_path));

            if let Some(source) = source_file {
                let source_id = format!("file:{source}");

                // Use efferent coupling info — but we need actual edge targets.
                // Cross-reference via the module metrics: look for modules this one imports.
                // For now, use afferent/efferent as annotations (actual edges need the graph data).
                let _ = source_id; // We'll add edges from metrics pairs below
            }
        }

        // Add edges between modules that have dependencies
        // We identify these from modules with non-zero coupling
        let module_files: std::collections::HashMap<&str, &str> = report
            .modules
            .iter()
            .map(|m| (m.module_name.as_str(), m.file_path.as_str()))
            .collect();

        // The Architect's DependencyGraph has edges, but we only have the report.
        // We can infer edges from modules with fan-out > 0 by looking at the full graph data.
        // For a cleaner approach, we'll just show the cycle edges (most impactful).
        for violation in &report.violations {
            if violation.rule == "circular_dependency" && violation.modules.len() >= 2 {
                for i in 0..violation.modules.len() {
                    let from = &violation.modules[i];
                    let to = &violation.modules[(i + 1) % violation.modules.len()];

                    let from_file = module_files.get(from.as_str()).copied();
                    let to_file = module_files.get(to.as_str()).copied();

                    if let (Some(src), Some(dst)) = (from_file, to_file) {
                        let src_id = format!("file:{src}");
                        let dst_id = format!("file:{dst}");
                        edges.push(json!({
                            "data": {
                                "id": format!("dep:{}:{}", from, to),
                                "source": src_id,
                                "target": dst_id,
                                "type": "cycle",
                                "label": "cycle",
                            }
                        }));
                    }
                }
            }
        }
    }

    let mut data = json!({
        "elements": {
            "nodes": nodes,
            "edges": edges,
        },
        "stats": {
            "files": graph.file_count(),
            "symbols": graph.symbol_count(),
        }
    });

    // Add architect stats if available
    if let Some(report) = architect_report {
        data["architect"] = json!({
            "score": report.score,
            "grade": report.grade,
            "violations": report.violations.len(),
            "topologicalDensity": topological_density,
        });
    }

    data
}

// ── X-Ray API (HCI Req 10: Killer Feature) ──────────────────────

/// Query parameters for the X-Ray endpoint.
#[derive(serde::Deserialize)]
struct XrayQuery {
    node: String,
}

/// Returns imports, importers, and a source preview for a file node.
async fn api_xray(
    Query(query): Query<XrayQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let ctx = state.ctx;
    let file_path = query.node;
    let project_root = ctx.project_root();

    let shared_graph = ctx.get_extension::<CodeGraph>();

    let result = tokio::task::spawn_blocking(move || {
        let mut imports: Vec<String> = Vec::new();
        let mut importers: Vec<String> = Vec::new();
        let mut preview = String::new();

        // 1. Get imports from CodeGraph (Import symbols in this file)
        let file_as_path = std::path::Path::new(&file_path);
        if let Some(ref graph) = shared_graph {
            if let Some(file) = graph.hoist(file_as_path) {
                for sym in &file.symbols {
                    if sym.kind == SymbolKind::Import {
                        let label = sym
                            .signature
                            .as_deref()
                            .unwrap_or(&sym.name);
                        imports.push(label.to_string());
                    }
                }
            }

            // 2. Find importers — scan all files for Import symbols that reference this file
            let target_stem = file_path
                .rsplit('/')
                .next()
                .unwrap_or(&file_path)
                .rsplit('.')
                .next_back()
                .unwrap_or(&file_path);
            for other in graph.all_files() {
                if other.path == file_path {
                    continue;
                }
                for sym in &other.symbols {
                    if sym.kind == SymbolKind::Import {
                        let sig = sym.signature.as_deref().unwrap_or(&sym.name);
                        if sig.contains(target_stem) {
                            let short = other
                                .path
                                .rsplit('/')
                                .next()
                                .unwrap_or(&other.path);
                            importers.push(short.to_string());
                            break; // one entry per file
                        }
                    }
                }
            }
        }

        // 3. Read first 15 lines of source
        let abs_path = if std::path::Path::new(&file_path).is_absolute() {
            std::path::PathBuf::from(&file_path)
        } else {
            project_root.join(&file_path)
        };
        if let Ok(content) = std::fs::read_to_string(&abs_path) {
            preview = content.lines().take(15).collect::<Vec<_>>().join("\n");
        }

        let short_file = file_path
            .rsplit('/')
            .next()
            .unwrap_or(&file_path)
            .to_string();

        json!({
            "file": short_file,
            "imports": imports,
            "importers": importers,
            "preview": preview,
        })
    })
    .await;

    match result {
        Ok(data) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json".to_string())],
            serde_json::to_string(&data).unwrap_or_default(),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("X-Ray failed: {e}"),
        )
            .into_response(),
    }
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

/// Classify coupling/instability into a physics metaphor for the visualizer.
///
/// - **rigid**: high coupling + low instability → monolith, hard to change.
/// - **gaseous**: almost no coupling → isolated island, potentially dead code.
/// - **fluid**: balanced coupling + moderate instability → healthy module.
/// - **none**: no data available.
fn physics_level(total_coupling: usize, instability: f64) -> &'static str {
    if total_coupling == 0 {
        return "gaseous";
    }
    if total_coupling >= 8 && instability < 0.3 {
        "rigid"
    } else if total_coupling <= 1 {
        "gaseous"
    } else {
        "fluid"
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

    /// Helper: bind port 0 to get an OS-assigned port, return (listener, port).
    async fn occupy_port() -> (tokio::net::TcpListener, u16) {
        let l = tokio::net::TcpListener::bind((LOCALHOST, 0u16))
            .await
            .unwrap();
        let port = l.local_addr().unwrap().port();
        (l, port)
    }

    #[tokio::test]
    async fn test_bind_with_retry_finds_free_port() {
        let listener = bind_with_retry(LOCALHOST, 0, true).await.unwrap();
        assert!(listener.local_addr().unwrap().port() > 0);
    }

    #[tokio::test]
    async fn test_bind_with_retry_skips_occupied() {
        let (_hold, occupied_port) = occupy_port().await;

        let listener = bind_with_retry(LOCALHOST, occupied_port, true)
            .await
            .unwrap();
        let actual_port = listener.local_addr().unwrap().port();
        assert!(
            actual_port > occupied_port,
            "Expected port > {occupied_port}, got {actual_port}"
        );
    }

    #[tokio::test]
    async fn test_bind_with_retry_exhausts_limit() {
        // Find a consecutive range of PORT_RETRY_LIMIT + 1 free ports.
        // We try multiple base ports until we can occupy the full range.
        let mut holders = Vec::new();
        let mut base_port = 0u16;

        'outer: for attempt_base in (40_000u16..50_000).step_by(20) {
            holders.clear();
            let mut all_bound = true;

            for offset in 0..=PORT_RETRY_LIMIT {
                match tokio::net::TcpListener::bind((LOCALHOST, attempt_base + offset)).await {
                    Ok(l) => holders.push(l),
                    Err(_) => {
                        all_bound = false;
                        break;
                    }
                }
            }

            if all_bound {
                base_port = attempt_base;
                break 'outer;
            }
        }

        assert!(
            !holders.is_empty() && holders.len() == (PORT_RETRY_LIMIT + 1) as usize,
            "Could not find a consecutive range of {} free ports",
            PORT_RETRY_LIMIT + 1
        );

        let result = bind_with_retry(LOCALHOST, base_port, true).await;
        assert!(result.is_err(), "Expected error when all ports exhausted");

        drop(holders);
    }

    #[tokio::test]
    async fn test_bind_without_retry_fails_immediately() {
        let (_hold, occupied_port) = occupy_port().await;

        let result = bind_with_retry(LOCALHOST, occupied_port, false).await;
        assert!(
            result.is_err(),
            "Expected immediate error with retry=false"
        );
    }
}
