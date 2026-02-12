# Changelog

## [1.0.0] — 2026-02-12

### The "Production-Ready" Release

SYNAPSEED graduates from prototype to production. Every subsystem has been hardened,
unified, and documented. The thinking layer between you and your LLM is now ready
for real-world codebases.

---

### Architecture (11 crates, 14 tools, 6 resources, 6 prompts)

- **synapseed-core** — Event bus, plugin trait, context extensions, telemetry init
- **synapseed-cortex** — Tree-sitter AST parsing (Rust, Python, JS), CodeGraph with parallel rayon indexing
- **synapseed-husk** — DLP shield with Aho-Corasick + regex detection, configurable custom rules via DNA
- **synapseed-root** — Command sentinel with deny-first policy enforcement
- **synapseed-chronos** — Git time-travel: blame, history, semantic commit tags, intent analysis
- **synapseed-search** — Tantivy full-text semantic index (RAM or disk-persistent)
- **synapseed-shadow-check** — Background `cargo check` with `MachineApplicable` quick-fix engine
- **synapseed-visualizer** — Live Cytoscape.js architecture dashboard with WebSocket + OTLP heatmap
- **synapseed-whisper** — Intent router: NLP classification → multi-subsystem orchestration
- **synapseed-telemetry-sink** — OTLP gRPC receiver (port 4317), ring-buffer SpanStore, hotspot ranking
- **synapseed-mcp** — Full MCP 2024-11-05 protocol: tools, resources, prompts, dynamic context injection

### New Features

- **Parallel AST Indexing** — `CodeGraph::index_directory()` uses rayon `par_iter` for multi-core parsing. Each thread creates its own tree-sitter parser; DashMap handles concurrent inserts. Measured ~3x speedup on 8-core machines with large codebases.

- **Git Intent Summary** — New `git_intent_summary` MCP tool (14th tool). Analyzes recent commits via `Historian::summarize_intent()`, groups them by semantic category (fix, feature, refactor, security, performance, test, docs), extracts conventional commit scopes, and returns a natural-language summary like: *"12 commits over 5 days: 4 feature (auth, router), 3 refactor, 2 fix, 2 docs, 1 test"*.

- **Tantivy Disk Persistence** — `SemanticIndex::open_or_create()` persists the full-text index to `.synapseed/index/` on disk. Enable via `search.persistence: true` in `dna.yaml`. Schema-mismatch detection auto-recreates the index. Fallback to RAM on any error.

- **Configurable DLP Rules** — Custom regex/literal patterns in `dna.yaml` under `dlp_custom_rules`. Each rule has `name`, `pattern`, and `action` (redact/deny/audit/allow). Merged with built-in Aho-Corasick defaults. Wired through `HuskPlugin::from_dna()` → `SecurityGuard::from_policy()`.

- **Configurable Visualizer Port** — `SYNAPSEED_VISUALIZER_PORT` env var or `visualizer_port` in DNA config. Priority: env > config > default 3000. `VisualizerPlugin::from_config()` constructor.

### Improvements

- **CLI/Service Unification** — MCP tools now reuse plugin-initialized subsystems via context extensions instead of creating fresh instances on every call:
  - `CodeGraph` → `Arc<CodeGraph>` registered by CortexPlugin
  - `Sentinel` → `Arc<Sentinel>` registered by RootPlugin
  - `SecurityGuard` → `Arc<SecurityGuard>` registered by HuskPlugin
  - Tools try `ctx.get_extension::<T>()` first, fall back to ephemeral instances for non-root paths

- **ProjectDna Extensions** — Three new config fields: `search: SearchConfig` (persistence toggle), `dlp_custom_rules: Vec<DlpRule>` (custom patterns), `visualizer_port: Option<u16>`. All with `#[serde(default)]` for backward compatibility. Cascading merge updated.

### Documentation

- **README** — Mermaid.js architecture diagram, custom security rules section, search persistence note, updated tool count (14), `SYNAPSEED_VISUALIZER_PORT` env var, fixed install path
- **VitePress docs** — Updated `mcp-tools.md` (14 tools + git_intent_summary), `search.md` (disk persistence), `configuration.md` (new DNA fields, custom DLP rules, env vars)

### Testing

- 34 tests passing across workspace (6 chronos, 7 whisper, 3 search, 3 shadow-check, 4 telemetry-sink, 7 integration MCP, 1 scenario, 3 doc-tests)
- Integration test validates 14 tools, 6 resources, 6 prompts
- Full MCP lifecycle: initialize → tools/list → tools/call → resources → prompts → ping
- DLP detection, unknown tool/method errors, pre-init rejection, resource reads, prompt expansion

### Technical Details

- **Dependencies added**: `rayon = "1"` (workspace + cortex crate)
- **Binary size**: <12 MB (release profile: opt-level 3, LTO, single codegen unit, stripped)
- **Zero network calls**: All processing local, all servers bound to 127.0.0.1
- **Minimum Rust version**: 1.75+

---

[1.0.0]: https://github.com/fabriziosalmi/synapseed/releases/tag/v1.0.0
