# SYNAPSEED

**Pure Rust Semantic AI Middleware — The Thinking Layer Between You and Your LLM.**

[![Build Status](https://img.shields.io/github/actions/workflow/status/fabriziosalmi/synapseed/ci.yml?branch=main)](https://github.com/fabriziosalmi/synapseed/actions)
[![Coverage](https://codecov.io/gh/fabriziosalmi/synapseed/branch/main/graph/badge.svg)](https://codecov.io/gh/fabriziosalmi/synapseed)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-stable-brightgreen.svg)](https://www.rust-lang.org/)
[![MCP Ready](https://img.shields.io/badge/MCP-Ready-blue)](https://modelcontextprotocol.io/)

---

Most AI coding agents treat your codebase as flat text — `grep` for search, `cat` for reading, zero security. This leads to hallucinations, broken imports, and leaked secrets.

**SYNAPSEED** parses code into an **AST**, indexes it semantically, scans for secrets in real-time, tracks git history with semantic tags, compiles in the background, analyzes architecture health, and visualizes everything live — all in a **single Rust binary** with **zero network calls**.

| Capability | Standard LLM Context | SYNAPSEED |
| :--- | :--- | :--- |
| Code access | `cat file.rs` (blind text) | AST skeleton with symbols and relationships |
| Search | Regex / grep | Tantivy FTS + vector embedding similarity |
| Security | None (leaked secrets) | DLP fail-closed with real-time redaction |
| Context | Zero (stateless) | Git time-travel + session continuity |
| Safety | Suggests `rm -rf /` | Command sentinel + Janitor dry-run default |
| Architecture | None | Dependency graph, coupling metrics, cycle detection |
| Visibility | None | Live graph + X-Ray Mode (Shift+hover) |
| Observability | None | OTLP telemetry receiver with heatmap |
| Startup | Blocks until indexed | Background indexing, port-hopping |
| Latency | High (network calls) | Zero-copy direct Rust (<10 ms) |

---

## Quick Start

```bash
git clone https://github.com/fabriziosalmi/synapseed.git
cd synapseed
cargo install --path bin/synapseed --force
synapseed --version
```

**Prerequisites:** Rust 1.75+, Git.

---

## Integration

### Claude Desktop

Add to `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) or `%APPDATA%\Claude\claude_desktop_config.json` (Windows):

```json
{
  "mcpServers": {
    "synapseed": {
      "command": "synapseed",
      "args": ["serve", "--project", "/path/to/your/project"],
      "env": { "RUST_LOG": "warn" }
    }
  }
}
```

### Claude Code

Add to `~/.claude/settings.json` or `.mcp.json` in your project root:

```json
{
  "mcpServers": {
    "synapseed": {
      "command": "synapseed",
      "args": ["serve", "--project", "."],
      "env": { "RUST_LOG": "warn" }
    }
  }
}
```

### VS Code / Cursor / Copilot

Add to `.vscode/mcp.json`:

```json
{
  "servers": {
    "synapseed": {
      "command": "synapseed",
      "args": ["serve", "--project", "${workspaceFolder}"],
      "env": { "RUST_LOG": "warn" }
    }
  }
}
```

See the [full integration guides](docs/integration/) for system prompt templates and advanced configuration.

---

## Architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│                        MCP JSON-RPC (stdio)                          │
├──────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌─────────┐  ┌──────┐  ┌──────┐  ┌─────────┐  ┌───────┐  ┌─────┐ │
│  │ Cortex  │  │ Husk │  │ Root │  │ Chronos │  │Search │  │ Gym │ │
│  │  (AST)  │  │(DLP) │  │(Cmd) │  │  (Git)  │  │(FTS)  │  │(RL) │ │
│  └────┬────┘  └──┬───┘  └──┬───┘  └────┬────┘  └───┬───┘  └──┬──┘ │
│       │          │         │            │           │          │     │
│  ┌────┴──────────┴─────────┴────────────┴───────────┴──────────┴──┐ │
│  │                SynapseContext (Event Bus + Sessions)            │ │
│  └────┬──────────┬─────────┬────────────┬──────────┬──────────┬───┘ │
│       │          │         │            │          │          │      │
│  ┌────┴────┐  ┌──┴───┐  ┌─┴──────┐  ┌──┴──┐  ┌───┴────┐  ┌─┴───┐ │
│  │ Shadow  │  │Whis- │  │Visuali-│  │Tele-│  │Janitor │  │Arch-│ │
│  │(Compile)│  │ per  │  │  zer   │  │metry│  │(Maint.)│  │itect│ │
│  └─────────┘  └──────┘  └────────┘  └─────┘  └────────┘  └─────┘ │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
   15 crates · Plugin architecture · Priority-based init · HCI-tuned
```

```mermaid
graph LR
    LLM["LLM / AI Agent"]
    MCP["MCP Server<br/>(JSON-RPC stdio)"]
    LLM <-->|"tools · resources · prompts"| MCP

    subgraph Plugins
        Cortex["Cortex<br/>AST + CodeGraph"]
        Husk["Husk<br/>DLP Shield"]
        Root["Root<br/>Sentinel"]
        Chronos["Chronos<br/>Git History"]
        Search["Search<br/>Tantivy FTS"]
        Whisper["Whisper<br/>Intent Router"]
    end

    MCP --> Cortex & Husk & Root & Chronos & Search & Whisper

    Cortex -->|"tree-sitter"| FS["File System"]
    Chronos -->|"git2"| Git["Git Repo"]
    Root -->|"policy eval"| Shell["Shell Commands"]
```

| Crate | Role |
| :--- | :--- |
| `synapseed-core` | Event bus, plugin trait, context, session persistence, HCI config |
| `synapseed-cortex` | Tree-sitter AST (Rust/Python/JS + 27 fallback), background indexing |
| `synapseed-husk` | DLP shield — Aho-Corasick + regex secret detection |
| `synapseed-root` | Command sentinel — policy-based command evaluation |
| `synapseed-chronos` | Git history with semantic commit tags and intent analysis |
| `synapseed-search` | Tantivy FTS + local vector embeddings (fastembed, cosine similarity) |
| `synapseed-shadow-check` | Background `cargo check` with severity filtering and adaptive debounce |
| `synapseed-visualizer` | Live Cytoscape.js dashboard with X-Ray Mode and port-hopping |
| `synapseed-whisper` | Intent router with query complexity analysis (Mentor Mode) |
| `synapseed-telemetry-sink` | OTLP gRPC receiver on port 4317, SpanStore, heatmap |
| `synapseed-gym` | RL sandbox — safe code evaluation with compilation + test feedback |
| `synapseed-janitor` | Autonomous maintenance — clippy + unused deps, validated proposals |
| `synapseed-architect` | Dependency graph, coupling metrics, cycle detection, scoring (A-F) |
| `synapseed-mcp` | MCP protocol handler — 20 tools, 9 resources, 6 prompts |

---

## MCP Tools (20)

| Tool | Tier | CLI Alias | Description |
| :--- | :--- | :--- | :--- |
| `ask` | PRIMARY | `ask_synapseed`, `whisper` | Intent-based orchestration — start here for any question |
| `hoist` | LOW-LEVEL | `get_code_skeleton` | Parse project AST and return symbol graph |
| `lookup` | LOW-LEVEL | `lookup_symbol` | Find function/struct/trait by name |
| `search` | LOW-LEVEL | `semantic_search` | Concept-based code search via Tantivy |
| `scan` | LOW-LEVEL | `scan_security` | DLP scan for secrets (API keys, passwords, private keys) |
| `check` | LOW-LEVEL | `check_command` | Evaluate shell command against security policy |
| `blame` | LOW-LEVEL | `git_history` | Semantic git blame and file history |
| `analyze` | LOW-LEVEL | `analyze_history` | Churn analysis, risk scoring, change patterns |
| `intent` | LOW-LEVEL | `git_intent_summary` | Summarize recent commit intent by category |
| `diagnostics` | LOW-LEVEL | `get_diagnostics` | Live compiler errors and warnings |
| `quickfix` | LOW-LEVEL | `apply_quick_fix` | Auto-apply compiler-suggested fixes |
| `consult` | LOW-LEVEL | `consult_architect` | Architecture guidance from project DNA config |
| `diagnose` | LOW-LEVEL | `project_diagnose` | Full system diagnostic across all subsystems |
| `reset-telemetry` | LOW-LEVEL | `reset_telemetry` | Clear telemetry span store and metrics |
| `train` | SPECIALIZED | `train_code` | Evaluate Rust code in isolated sandbox (The Gym) |
| `janitor` | SPECIALIZED | `janitor_run_now` | Scan for clippy warnings and unused deps |
| `janitor-fix` | SPECIALIZED | `janitor_apply_fix` | Apply a Janitor fix (dry-run preview by default) |
| `architect` | SPECIALIZED | `architect_analyze` | Structural health analysis (score, cycles, coupling) |
| `oracle` | SPECIALIZED | `oracle_fix_docs` | Auto-repair drifted documentation (version, counts) |
| `similar` | SPECIALIZED | `semantic_similarity` | Vector embedding similarity search |

## MCP Resources (9)

| URI | Description |
| :--- | :--- |
| `synapseed://status` | Project status (state, metrics, plugins) |
| `synapseed://dna` | Project DNA configuration |
| `synapseed://security/policy` | Active security policy rules |
| `synapseed://diagnostics/active` | Current compiler diagnostics |
| `synapseed://visualizer/url` | Visualizer dashboard URL |
| `synapseed://telemetry/hotspots` | Top-10 performance hotspots from OTLP spans |
| `synapseed://janitor/proposals` | Janitor fix proposals |
| `synapseed://architect/health` | Architecture health score and violations |
| `synapseed://consistency` | Consistency Oracle report (drift detection) |

## MCP Prompts (6)

- **`onboard_project`** — Generate a comprehensive project onboarding guide
- **`security_audit`** — Deep security audit across DLP, commands, and architecture
- **`refactor_module`** — Analyze and plan a module refactoring
- **`debug_error`** — Systematic error debugging with compiler + history context
- **`architecture_review`** — Full architecture review with risk assessment
- **`optimize_hotspots`** — Analyze telemetry hotspots and suggest optimizations

---

## CLI Commands

Every MCP tool is available as a CLI command. Legacy MCP names (e.g. `ask_synapseed`, `get_code_skeleton`) are accepted as visible aliases. Unrecognized input is treated as an `ask` query.

```bash
# ── Quick Ask (default fallback) ──
synapseed "why is login broken?"     # Shorthand for: synapseed ask "..."

# ── Server & System ──
synapseed serve --project .          # Start MCP server (stdio)
synapseed init --project .           # Initialize all plugins and broadcast event
synapseed status --project .         # Runtime metrics and system status
synapseed diagnose --project .       # Full system diagnostic

# ── Code Analysis ──
synapseed hoist                      # Index project and print AST skeleton
synapseed hoist src/                 # Index a specific subdirectory
synapseed lookup <name> --project .  # Find symbol by name across the project
synapseed search "auth login" -l 10  # Semantic search via Tantivy
synapseed similar "error handling"   # Vector embedding similarity search
synapseed ask "why is login broken?" # Ask SYNAPSEED (orchestrates everything)

# ── Security ──
synapseed scan -c "secret=..."       # DLP scan for sensitive data
synapseed scan --content "..." -m dlp  # DLP-only scan mode
echo "data" | synapseed scan         # Scan from stdin
synapseed check "cargo test"         # Evaluate command against security policy

# ── Git ──
synapseed history --limit 20         # Git history with semantic commit tags
synapseed blame <file> -s 1 -e 30   # Git blame for file line range
synapseed analyze <file>             # Churn analysis, hotspots, risk scoring
synapseed intent --limit 10          # Semantic commit intent summary

# ── Compiler & Maintenance ──
synapseed diagnostics                # Live compiler errors and warnings
synapseed quickfix <file> <code>     # Auto-apply compiler-suggested fix
synapseed janitor                    # Scan clippy warnings + unused deps
synapseed janitor-fix <id> --confirm # Apply a janitor fix proposal

# ── Architecture ──
synapseed architect --refresh        # Structural health analysis (A-F score)
synapseed consult "which runtime?"   # Consult architecture policy (DNA)
synapseed oracle                     # Auto-repair drifted documentation

# ── Sandbox ──
synapseed train src.rs --adversarial # Evaluate Rust code in the Gym sandbox
synapseed reset-telemetry            # Clear OTLP telemetry data
```

---

## Self-Telemetry (Dogfooding)

SYNAPSEED can observe its own performance by sending tracing spans to its own OTLP receiver:

```bash
SYNAPSEED_SELF_TELEMETRY=1 synapseed serve --project .
```

This enables a feedback loop: SYNAPSEED operations emit spans via `BatchSpanProcessor` to `localhost:4317`, where the TelemetrySink ingests them into SpanStore. The Visualizer then colors nodes by latency — red for hot, yellow for warm, green for cool.

---

## Configuration

Create `.synapseed/dna.yaml` in your project root (or `~/.config/synapseed/dna.yaml` for user-level defaults):

```yaml
workspace_strategy: monorepo

naming:
  core_crate: core
  bin_name: my-app

preferred_libs:
  async: tokio
  json: serde_json
  error: thiserror
  http: axum

plugins:
  - cortex
  - husk
  - root
  - chronos
  - search
  - visualizer
  - shadow
  - whisper
  - telemetry

dlp_level: standard          # off | low | standard | strict | paranoid
visualizer_port: 3000        # override with SYNAPSEED_VISUALIZER_PORT env var

hci:
  background_indexing: true   # Non-blocking startup (Cortex indexes in background)
  port_retry: true            # Visualizer auto-retries next port if taken
  adaptive_linting: true      # Shadow-check debounce escalates during rapid edits
  mentor_mode: true           # Response depth adapts to query complexity
  session_persistence: true   # Resume context across MCP restarts
  memory_ceiling_files: 10000 # Cap indexed files to limit memory usage
```

All fields are optional — omitted fields use sensible defaults. Project-level config overrides user-level config. See [`examples/dna.yaml`](examples/dna.yaml) for a full annotated example.

### Search Index

By default, the Tantivy search index runs **in-memory** (RAM-only) for instant startup. On large codebases, enable disk persistence to avoid re-indexing on every MCP server restart:

```yaml
# .synapseed/dna.yaml
search:
  persistence: true  # persists index to .synapseed/index/
```

### Environment Variables

| Variable | Default | Description |
| :--- | :--- | :--- |
| `RUST_LOG` | `info` | Log level filter |
| `SYNAPSEED_LOG_FORMAT` | `compact` | Log format (`compact` or `json`) |
| `SYNAPSEED_SELF_TELEMETRY` | `0` | Enable self-instrumentation (`1` to enable) |
| `SYNAPSEED_VISUALIZER_PORT` | `3000` | Override visualizer dashboard port |

---

## Security

SYNAPSEED enforces a **defense-in-depth, fail-closed** security model:

1. **DLP Shield (Husk)** — Aho-Corasick + regex scanning blocks API keys, passwords, private keys, and PII from ever reaching the LLM
2. **Command Sentinel (Root)** — Deny-first policy evaluation blocks destructive commands
3. **Network Isolation** — All servers bind to `127.0.0.1` only, zero outbound calls
4. **Process Boundary** — No arbitrary subprocess spawning, read-only AST analysis

### Custom Security Rules

Add custom DLP rules in `.synapseed/dna.yaml`:

```yaml
dlp_level: strict

dlp_custom_rules:
  - name: internal_api_key
    pattern: "INTERNAL-[A-Z0-9]{16}"
    action: redact
  - name: corporate_email
    pattern: "\\b[\\w.]+@corp\\.example\\.com\\b"
    action: audit
```

Rule actions: `redact` (replace with `[REDACTED]`), `deny` (block entirely), `audit` (log only), `allow` (skip).

### DLP Whitelist

Suppress false-positive DLP findings with regex patterns in `.synapseed/dna.yaml`:

```yaml
dlp_whitelist:
  - "(?i)token\\s*[:=]\\s*[A-Z]\\w+"   # CancellationToken, etc.
  - "(?i)shutdown_token"                 # Rust async shutdown pattern
```

Built-in defaults already suppress `CancellationToken` and `shutdown_token` patterns common in Rust codebases.

The Sentinel uses a **deny-first** model: any command matching a deny rule is blocked regardless of allow rules. Default deny list includes `rm -rf`, `curl | sh`, `chmod 777`, `mkfs`, and other destructive patterns.

---

## Documentation

Full documentation is available in the [docs/](docs/) directory, built with VitePress:

```bash
cd docs
npm install
npm run dev
# Open http://localhost:5173
```

Sections: [Guide](docs/guide/) · [Architecture](docs/architecture/) · [Features](docs/features/) · [MCP Reference](docs/reference/) · [Integration](docs/integration/) · [Security](docs/security/)

---

## Contributing

1. Fork the repo
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

---

## License

Distributed under the MIT License. See `LICENSE` for more information.

---

**Built with Rust by [Fabrizio Salmi](https://github.com/fabriziosalmi)**
