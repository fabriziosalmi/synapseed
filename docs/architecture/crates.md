# Crate Map

SYNAPSEED is a Cargo workspace with 11 library crates and 1 binary crate.

## Overview

| Crate | Description | Priority | Key Dependencies |
| :--- | :--- | :--- | :--- |
| `synapseed-core` | Shared types, traits, event bus | — | tokio, serde, tracing |
| `synapseed-cortex` | AST parsing & code graph | 50 | tree-sitter (Rust/Python/JS) |
| `synapseed-husk` | DLP & secret detection | 10 | aho-corasick, regex |
| `synapseed-root` | Command sandbox & sentinel | 20 | regex, synapseed-husk |
| `synapseed-chronos` | Git history & blame | 100 | git2 (libgit2) |
| `synapseed-search` | Tantivy semantic search | 160 | tantivy, synapseed-cortex |
| `synapseed-shadow-check` | Background compiler | 150 | tokio (process) |
| `synapseed-visualizer` | Live dashboard | 250 | axum, rust-embed, notify |
| `synapseed-whisper` | Intent router | 999 | all subsystems |
| `synapseed-telemetry-sink` | OTLP gRPC receiver | 200 | tonic, prost, opentelemetry-proto |
| `synapseed-mcp` | MCP protocol bridge | — | all subsystems |
| `synapseed-cli` | Binary entry point | — | all crates, clap |

## Core (`synapseed-core`)

The foundation layer. Defines:

- `SynapsePlugin` trait
- `SynapseContext` (shared state + event bus)
- `SynapseEvent` enum
- `ProjectDna` configuration
- `ProjectState` detection
- Telemetry initialization (with optional OTLP self-instrumentation)

## Brain Layer

### Cortex
Multi-language AST parsing via tree-sitter. Produces `FileStructure` and `Symbol` types. Supports Rust, Python, and JavaScript.

### Search
Builds a Tantivy full-text index over AST symbols. Indexes name, signature, doc comments, and body content for concept-based search.

## Security Layer

### Husk
Ultra-fast DLP engine using Aho-Corasick for multi-pattern matching. Detects AWS keys, GitHub tokens, generic secrets, and private key markers.

### Root
Command execution sandbox with regex-based whitelist/blacklist policies. Fail-closed by default.

## Analysis Layer

### Chronos
Git intelligence via libgit2. Provides blame, commit history, churn analysis, co-change detection, and semantic commit classification (fix/revert/refactor/security).

### Shadow Check
Spawns `cargo check --message-format=json` in the background. Parses compiler output into structured diagnostics with quick-fix suggestions.

## Infrastructure Layer

### Visualizer
Axum HTTP server serving an embedded Cytoscape.js dashboard. WebSocket connection for live file-change updates and telemetry heatmap rendering.

### Telemetry Sink
gRPC server (tonic) implementing the OTLP TraceService. Receives spans, resolves `code.file.path` + `code.line.number` to source symbols, stores in a ring buffer (1000 spans).

## Orchestration Layer

### Whisper
Intent Router that classifies natural-language queries and orchestrates multiple subsystems in a single call. Reduces LLM roundtrips by providing enriched context objects.

## Bridge Layer

### MCP
JSON-RPC 2.0 server over stdin/stdout. Exposes 13 tools, 6 resources, and 6 prompt templates. Handles initialization handshake, method routing, and error responses.
