# Changelog

## [3.2.0] — 2026-02-12

### "CLI Polish" Release

Quality polish: CLI/MCP argument parity, stderr-only telemetry, default ask
fallback, visible aliases, and dead code cleanup.

---

### Architecture (15 crates, 20 tools, 9 resources, 6 prompts)

Score: **97/100 (Grade A)** — 161 modules, 58 edges.
93 tests passing, 0 failures.

### Fixes

- **Argument parity (scan)** — Renamed CLI `--text` to `--content` to match MCP
  tool schema. Added `--mode` flag (all/dlp/patterns). Scan now routes through
  `cmd_mcp()` bridge for zero logic duplication.

- **Argument parity (hoist)** — Added optional positional `path` argument to CLI
  `hoist` command. `synapseed hoist src/` now works. Routes through `cmd_mcp()`.

- **Telemetry to stderr** — `init_telemetry()` now writes to stderr in both
  compact and JSON modes. Tracing output no longer contaminates stdout for any
  CLI command.

- **Visible aliases** — All `alias` attributes changed to `visible_alias`.
  Legacy MCP names now appear in `synapseed --help` output for discoverability.

- **Default ask fallback** — `synapseed "why is login broken?"` now works as
  shorthand for `synapseed ask "..."`. Unrecognized subcommands are interpreted
  as `ask` queries via Clap `external_subcommand`.

### Removed

- `cmd_scan()` local handler — replaced by `cmd_mcp("scan", ...)` bridge
- `cmd_hoist()` local handler — replaced by `cmd_mcp("hoist", ...)` bridge
- `SecurityGuard` import (no longer needed in CLI binary)

### Technical Details

- **Files changed**: 4 (`bin/synapseed/src/main.rs`, `crates/core/src/telemetry.rs`,
  `README.md`, `CHANGELOG.md`)
- **Zero new dependencies**
- **Net code reduction**: ~40 lines removed (dead handlers replaced by bridge calls)

---

## [3.1.0] — 2026-02-12

### "Fuzzy & Resilient" Release

CLI and MCP tool names are now perfectly aligned. Every MCP tool has a CLI
counterpart, every legacy name works as an alias, and typos are auto-corrected.

---

### Architecture (15 crates, 20 tools, 9 resources, 6 prompts)

Score: **97/100 (Grade A)** — 161 modules, 58 edges.
78 tests passing, 0 failures.

### Breaking Changes (MCP tool names)

All 20 MCP tools have been renamed to short, CLI-aligned canonical names.
Legacy names continue to work as backward-compatible aliases.

| Old Name | New Name |
| :--- | :--- |
| `get_code_skeleton` | `hoist` |
| `lookup_symbol` | `lookup` |
| `scan_security` | `scan` |
| `check_command` | `check` |
| `git_history` | `blame` |
| `project_diagnose` | `diagnose` |
| `consult_architect` | `consult` |
| `semantic_search` | `search` |
| `get_diagnostics` | `diagnostics` |
| `analyze_history` | `analyze` |
| `apply_quick_fix` | `quickfix` |
| `ask_synapseed` | `ask` |
| `git_intent_summary` | `intent` |
| `train_code` | `train` |
| `reset_telemetry` | `reset-telemetry` |
| `janitor_run_now` | `janitor` |
| `janitor_apply_fix` | `janitor-fix` |
| `architect_analyze` | `architect` |
| `oracle_fix_docs` | `oracle` |
| `semantic_similarity` | `similar` |

### New Features

- **13 new CLI subcommands** — Every MCP-only tool now has a CLI counterpart:
  `ask`, `search`, `diagnostics`, `analyze`, `quickfix`, `intent`, `train`,
  `reset-telemetry`, `janitor`, `janitor-fix`, `architect`, `consult`,
  `oracle`, `similar`. Uses `handle_tool_call()` bridge for zero logic
  duplication.

- **CLI aliases** — All 7 existing CLI commands accept their MCP tool name
  as alias (e.g. `synapseed get_code_skeleton` = `synapseed hoist`).
  Extra mnemonic: `synapseed whisper` = `synapseed ask`.

- **Fuzzy tool dispatch** — Levenshtein-based auto-correction in MCP
  `handle_tool_call()`. Edit distance ≤ 3: auto-executes with
  "Did you mean 'X'?" prefix. Distance > 3: error with suggestion +
  full available tool list. No external dependencies.

- **`init_full_context()` helper** — Extracted from `cmd_serve()` to
  initialize all 12 plugins for CLI-to-MCP bridge commands.

### Technical Details

- **Files changed**: 3 (`crates/mcp/src/tools/mod.rs`,
  `bin/synapseed/src/main.rs`, `bin/synapseed/tests/integration_mcp.rs`)
- **Dispatch architecture**: `resolve_tool_name()` → `dispatch_tool()` with
  `TOOL_NAMES` const array for canonical names
- **Zero new dependencies**: Levenshtein implemented inline (~15 lines)

---

## [2.2.1] — 2026-02-12

### Security & Hardening Patch

10 fixes across 9 crates — security vulnerabilities, logic bugs, and documentation drift.

---

### Security Fixes

- **CRITICAL: XSS in Visualizer** — Replaced inline `onclick` attribute in `panels.js` with
  event delegation (`data-focus-node` + `addEventListener`), preventing quote injection in
  symbol IDs.
- **Sentinel hardening** — Added 4 deny rules: `sudo`, `eval`, `curl|bash`, `LD_PRELOAD`.
- **Security pattern scanner** — Added `outerHTML`, `insertAdjacentHTML`, `writeln` to XSS
  detection; tightened path traversal `.join()` regex to require path-like context.

### Bug Fixes

- **Proptest fuzzer** — Replaced invalid `\PC{0,100}` regex strategy with `any::<String>()`.
- **Sandbox corruption** — `unwrap_or_default()` on source read replaced with proper error
  propagation, preventing silent state corruption during adversarial mutation testing.
- **Oracle doc fixer** — `replace()` → `replace_all()` for crate/tool/resource count patching;
  silent `unwrap_or("0.0.0")` fallback replaced with `warn!()` + early return.
- **Epoch subtraction** — `newer.epoch - older.epoch` → `saturating_sub()` to prevent panic
  on git clock skew.
- **Dead code cleanup** — Removed unused `matched_text` field from DLP `Finding` struct
  (eliminated compiler warning).

### Documentation

- Removed stale "<12MB binary" claims from 4 files (index.md, introduction.md, installation.md,
  CHANGELOG.md).

---

## [2.2.0] — 2026-02-13

### "The Physics Engine" Release

SYNAPSEED gains mathematical depth: six new analytical capabilities that quantify
structural density, temporal decay, convergence dynamics, mutation resilience,
cross-artifact consistency, and security pattern recognition.

---

### Architecture (15 crates, 19 tools, 9 resources, 6 prompts)

Score: **97/100 (Grade A)** — 131+ modules, 44 edges, 1 remaining violation.
89 tests passing, 0 failures.

### New Features

- **Topological Density** (#15) — `D = E / (V × (V − 1))` directed graph density
  metric in `synapseed-architect`. Density anomaly detection in linter (high > 0.5,
  low < 0.02 with ≥10 modules). Configurable thresholds via DNA. Wired into
  `synapseed://architect/health` resource and architecture score (−5 penalty for
  density > 0.5).

- **Temporal Decay** (#16) — Exponential decay on Chronos hotspot scores:
  `raw_score × e^(−λ × days)` (λ default 0.01). Temporal boost in search results:
  `score × (0.7 + 0.3 × e^(−λ × age_days))` with `last_modified_epoch` Tantivy
  field. Configurable `temporal_decay_lambda` via DNA `search` config.

- **Convergence Rate** (#17) — Fix-chain detection in Chronos: consecutive fix
  commits within 48h window. `convergence_rate = 1.0 − (fix_chains / total)`,
  `rigidity = fix_chains / total`. Exposed in Whisper `HistoryContext` for
  intelligent routing.

- **Adversarial Sandbox** (#18) — `Saboteur` mutation engine in `synapseed-gym`
  with 5 strategies (ArithmeticSwap, BooleanNegate, BoundaryShift, ReturnRemove,
  StatementDelete). Max 20 mutations per eval. `train_code(adversarial: true)` runs
  cargo check + cargo test per mutant. `mutation_score = detected / total` blended
  into Gym report score.

- **Consistency Oracle** (#19) — `synapseed_core::oracle` cross-references
  Cargo.toml workspace members vs filesystem, README feature mentions, docs index
  link validity, and crate description completeness. New `synapseed://consistency`
  MCP resource returning scored consistency report.

- **Security Patterns** (#20) — `CodePatternScanner` in `synapseed-husk` with
  regex-based detection for SQL injection, XSS, command injection, and path
  traversal (14 patterns across 4 categories). `scan_security(mode: "all"|"dlp"|"patterns")`
  MCP tool now supports dual-mode operation combining DLP + code pattern scanning.

### MCP Changes

- New resource: `synapseed://consistency` — project-wide consistency report
- Updated tool: `scan_security` — added `mode` parameter (all/dlp/patterns)
- Updated tool: `train_code` — added `adversarial` boolean parameter
- Updated resource: `synapseed://architect/health` — includes `topological_density`

---

## [2.1.0] — 2026-02-12

### "The Hardening" Release

SYNAPSEED hardens its internals: graceful shutdown, expanded test coverage, tighter
API visibility, adversarial fuzzing, and a structural refactoring pass that
eliminated all three monolith files.

---

### Architecture (15 crates, 19 tools, 8 resources, 6 prompts)

Score: **97/100 (Grade A)** — 131 modules, 44 edges, 1 remaining violation.
93 tests passing, 0 failures.

### New Features

- **Graceful Shutdown** (#12) — `CancellationToken` propagation through all async
  plugins (Cortex, Visualizer, Shadow-Check, Telemetry-Sink). `AtomicBool` shutdown
  flag on `SynapseContext`. Ctrl-C triggers coordinated cleanup across all subsystems.

- **Adversarial Fuzzing** (#6) — `train_code(fuzz: true)` auto-generates proptest
  property-based tests for all public functions. `FuzzGenerator` parses function
  signatures and generates type-appropriate strategies (u8..u64, String, Vec, Option).
  Integrated into Gym sandbox evaluation pipeline.

- **Architect Crate** — New `synapseed-architect` crate: dependency graph analysis,
  coupling metrics (Ce/Ca/Instability), cycle detection, god object detection,
  `LinterConfig` from DNA, blueprint report generation. `architect_analyze` MCP tool
  with `ReportStore` caching.

- **Janitor Crate** — New `synapseed-janitor` crate: automated clippy scan, unused
  dependency detection, fix proposal system with dry-run preview. Background async
  scanning with `ProposalStore` and atomic scan-in-progress guard.

### Improvements

- **Test Coverage** (#13) — 93 tests (up from 78). Added:
  - 4 port-hopping tests for `bind_with_retry()` (extracted from `start()`)
  - 3 async background indexing tests for `CortexPlugin`
  - First `#[tokio::test]` async tests in the codebase

- **API Visibility** (#14) — Tightened `pub` exports across all crates.
  `pub(crate)` for internal functions, `pub(super)` for module-private helpers.

### Structural Refactoring

- **graph.js Split** (#21) — God object (921 lines, 81 symbols) split into 9 focused
  modules: `constants.js`, `styles.js`, `layout.js`, `panels.js`, `events.js`,
  `search.js`, `xray.js`, `api.js`, `graph.js` (boot, 40 lines). Served via dynamic
  `/{name}.js` axum route. XSS safety preserved (`esc()` in `constants.js`).

- **tools.rs Split** (#23) — MCP tool monolith (1150 lines, 19 tools) split into
  `tools/mod.rs` (429 lines: schema registry + dispatch + helpers) + 12 sub-modules.
  Each tool file exports `pub(super) fn tool_xxx()`.

- **router.rs Split** (#22) — Whisper intent router (720 lines) split into
  `router/mod.rs` (530 lines: classify + extract + build context + tests) + 4 gather
  modules (`diagnostics.rs`, `history.rs`, `code.rs`, `security.rs`).

### Documentation

- **"The Quantum Loop"** — New `docs/guide/workflow.md`: operational playbook for
  high-velocity development using SYNAPSEED's 19 MCP tools. Includes tool tier
  classification, 5-step iteration cycle, ready-to-use prompt pack, anti-patterns,
  and the meta-loop (SYNAPSEED managing SYNAPSEED).

### Bug Fixes

- **Flaky port test** — `test_bind_with_retry_exhausts_limit` now deterministically
  finds a consecutive range of free ports instead of relying on OS-assigned ports.

- **Dependabot security alerts** — Resolved all outstanding dependency advisories.

---

[2.1.0]: https://github.com/fabriziosalmi/synapseed/compare/v2.0.2...v2.1.0

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
- **Release profile**: opt-level 3, LTO, single codegen unit, stripped
- **Zero network calls**: All processing local, all servers bound to 127.0.0.1
- **Minimum Rust version**: 1.75+

---

[1.0.0]: https://github.com/fabriziosalmi/synapseed/releases/tag/v1.0.0
