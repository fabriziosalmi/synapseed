# Visualizer — Archived

::: warning FEATURE ARCHIVED
The live visualization dashboard was **removed in v4.1.0** to reduce dependencies and simplify the codebase. The concepts described below remain architecturally relevant for understanding how SYNAPSEED can integrate with external visualization tools.
:::

## Previous Implementation

The **Visualizer** previously served an interactive web dashboard that rendered your codebase as a live graph. Files were containers, symbols were nodes, and everything updated in real-time.

## Alternative: VS Code Extension

For real-time project insights, use the [SYNAPSEED VS Code Extension](/integration/vscode-extension) which provides:
- **9 sidebar panels**: Status, Metrics, Diagnostics, Architecture Health, Git History, Security, Consistency, Janitor, Telemetry
- **Live updates**: Auto-refresh on file save
- **Commands**: Ask questions, run janitor scans, refresh all data
- **Status bar integration**: Build status with click-to-refresh

## Alternative: External Visualization Tools

SYNAPSEED's data can be consumed by external visualization tools through:
- **MCP Resources**: `synapseed://status`, `synapseed://architect/health`, `synapseed://telemetry/hotspots`
- **CLI Commands**: `synapseed architect --refresh`, `synapseed status`, `synapseed diagnose`
- **JSON Output**: All commands support `--format json` for programmatic consumption

---

## Historical Dashboard Features

The original dashboard (v1.0-v4.0) included:

- **Interactive graph** — Zoom, pan, and click on nodes
- **Symbol coloring** — Nodes colored by type (functions, structs, enums, etc.)
- **Live updates** — File changes pulse the affected nodes via WebSocket
- **Telemetry heatmap** — Runtime hotspots rendered as colored borders
- **Activity log** — Timestamped change history
- **Controls** — Refresh (re-index), Fit (center), Activity toggle

### Color Legend

| Color | Symbol Type |
| :--- | :--- |
| Green | Functions |
| Cyan | Methods |
| Blue | Structs / Classes / Interfaces |
| Purple | Enums |
| Orange | Modules / Constants |
| Yellow | Variables |
| Gray | Imports |

### Heatmap Colors

When telemetry data is available:

| Border Color | Meaning |
| :--- | :--- |
| Red (#f85149) | Hot — avg > 200ms |
| Yellow (#d29922) | Warm — avg 50–200ms |
| Green (#7ee787) | Cool — avg < 50ms |

## Technical Stack

- **Server:** Axum 0.7 with WebSocket support
- **Frontend:** Cytoscape.js (embedded via rust-embed)
- **File watching:** `notify` crate for filesystem events
- **Layout:** CoSE (Compound Spring Embedder) algorithm

## Architecture

```
Axum Server (:3000)
  ├── GET /           → index.html (embedded)
  ├── GET /graph.js   → Cytoscape frontend (embedded)
  ├── GET /api/graph  → JSON graph data (nodes + edges)
  └── GET /ws         → WebSocket for live events
```

The `/api/graph` endpoint:
1. Indexes the project via Cortex
2. Reads hotspot data from SpanStore (if available)
3. Builds Cytoscape.js elements (nodes + edges)
4. Returns JSON with stats

## WebSocket Events

```json
// File change event
{"type": "file_changed", "path": "src/main.rs", "kind": "modified"}

// Telemetry update event
{"type": "telemetry_update", "spans_received": 5, "hotspot_file": "src/auth.rs"}
```
