# Crate Map

SYNAPSEED is a Cargo workspace with 14 library crates and 1 binary crate.

## Overview

| Crate | Description | Priority | Key Dependencies |
| :--- | :--- | :--- | :--- |
| `synapseed-core` | Shared types, traits, event bus, consistency oracle | — | tokio, serde, tracing |
| `synapseed-cortex` | AST parsing & code graph | 50 | tree-sitter (Rust/Python/JS) |
| `synapseed-husk` | DLP, secret detection & code pattern scanning | 10 | aho-corasick, regex |
| `synapseed-root` | Command sandbox & sentinel | 20 | regex, synapseed-husk |
| `synapseed-chronos` | Git history, temporal decay & convergence | 100 | git2 (libgit2) |
| `synapseed-search` | Tantivy semantic search with temporal boost | 160 | tantivy, synapseed-cortex |
| `synapseed-shadow-check` | Background compiler | 150 | tokio (process) |
| `synapseed-architect` | Architecture analysis & density metrics | — | petgraph, synapseed-cortex |
| `synapseed-gym` | Code evaluation sandbox & adversarial testing | — | tempfile, synapseed-core |
| `synapseed-janitor` | Automated clippy & dependency cleanup | — | synapseed-core |
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
- `ConsistencyOracle` — cross-references workspace members, README, docs, and crate metadata
- Telemetry initialization (with optional OTLP self-instrumentation)

## Brain Layer

### Cortex
Multi-language AST parsing via tree-sitter. Produces `FileStructure` and `Symbol` types. Supports Rust, Python, and JavaScript.

### Search
Builds a Tantivy full-text index over AST symbols. Indexes name, signature, doc comments, body content, and `last_modified_epoch` for concept-based search with temporal boost.

## Security Layer

### Husk
Ultra-fast DLP engine using Aho-Corasick for multi-pattern matching. Detects AWS keys, GitHub tokens, generic secrets, and private key markers. Also includes `CodePatternScanner` for static detection of SQL injection, XSS, command injection, and path traversal patterns.

### Root
Command execution sandbox with regex-based whitelist/blacklist policies. Fail-closed by default.

## Analysis Layer

### Chronos
Git intelligence via libgit2. Provides blame, commit history, churn analysis with exponential temporal decay, co-change detection, semantic commit classification (fix/revert/refactor/security), convergence rate tracking, and rigidity metrics.

### Shadow Check
Spawns `cargo check --message-format=json` in the background. Parses compiler output into structured diagnostics with quick-fix suggestions.

## Infrastructure Layer

### Visualizer
Axum HTTP server serving an embedded Cytoscape.js dashboard. WebSocket connection for live file-change updates and telemetry heatmap rendering.

### Telemetry Sink
gRPC server (tonic) implementing the OTLP TraceService. Receives spans, resolves `code.file.path` + `code.line.number` to source symbols, stores in a ring buffer (1000 spans).

## Quality & Evaluation Layer

### Architect
Structural health analysis via petgraph. Builds the module dependency graph, computes coupling metrics (Ce/Ca/Instability), topological density `D = E / (V × (V − 1))`, cycle detection, god object detection, and layer violation linting. Reports are cached in `ReportStore`.

### Gym
Isolated Rust code evaluation sandbox. Compiles, tests, and benchmarks code in a tempdir. Includes the `Saboteur` adversarial mutation engine with 5 strategies (ArithmeticSwap, BooleanNegate, BoundaryShift, ReturnRemove, StatementDelete) for mutation testing.

### Janitor
Automated maintenance: runs clippy scans and unused dependency detection. Generates fix proposals with UUIDs, supports dry-run preview, and auto-reverts on compilation failure.

## Orchestration Layer

### Whisper
Intent Router that classifies natural-language queries and orchestrates multiple subsystems in a single call. Reduces LLM roundtrips by providing enriched context objects including convergence rate, rigidity metrics, and SID (Semantic Information Density). Adapts output format based on model tier (Atomic/Molecular/Galactic) and session phase (Discovery/Implementation/Stabilization).

## Bridge Layer

### MCP
JSON-RPC 2.0 server over stdin/stdout. Exposes 22 tools, 9 resources, and 6 prompt templates. Handles initialization handshake with client fingerprinting (auto-detects model tier), momentum tracking, method routing, and error responses.
