# Visualizer — Live Dashboard

The **Visualizer** serves an interactive web dashboard that renders your codebase as a live graph. Files are containers, symbols are nodes, and everything updates in real-time.

## Dashboard

Open `http://localhost:3000` when SYNAPSEED is running in serve mode.

### Features

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
