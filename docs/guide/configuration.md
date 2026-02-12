# Configuration

## Project DNA

SYNAPSEED uses a cascading configuration system called **DNA** (Dynamic Navigation Architecture). Configuration is loaded with the following priority:

1. **Project-level** — `.synapseed/dna.yaml` (highest priority)
2. **User-level** — `~/.config/synapseed/dna.yaml`
3. **Embedded defaults** (lowest priority)

## Configuration File

Create `.synapseed/dna.yaml` in your project root:

```yaml
# Workspace layout strategy
workspace_strategy: monorepo

# Preferred libraries for architecture guidance
preferred_libs:
  async: tokio
  json: serde_json
  error: thiserror
  cli: clap
  http: reqwest

# Naming conventions
naming:
  core_crate: core
  bin_name: synapseed

# Enabled plugins
plugins:
  - cortex
  - husk
  - root
  - chronos

# DLP sensitivity level
dlp_level: standard

# Custom DLP rules
dlp_custom_rules:
  - name: internal_id
    pattern: 'INTERNAL-\d{6}'
    action: redact

# Search index settings
search:
  persistence: true
  temporal_decay_lambda: 0.01

# Architect settings
architect:
  density_high_threshold: 0.5
  density_low_threshold: 0.02
  density_low_min_modules: 10

# Visualizer dashboard port
visualizer_port: 3000
```

## Fields Reference

### `workspace_strategy`

Strategy for workspace layout. Used by the architect tool to provide guidance.

- `monorepo` — Single repository with multiple crates (default)
- `polyrepo` — Multiple repositories

### `preferred_libs`

A key-value map of technology categories to preferred libraries. The `consult` MCP tool uses this to guide the LLM.

### `naming`

- `core_crate` — Name of the core/shared crate
- `bin_name` — Name of the main binary

### `plugins`

List of enabled plugins. Available plugins:

| Plugin | Description |
| :--- | :--- |
| `cortex` | AST parsing & code graph |
| `husk` | DLP & secret scanning |
| `root` | Command sentinel |
| `chronos` | Git history analysis |

### `dlp_level`

Controls how aggressively the DLP engine scans content.

| Level | Behavior |
| :--- | :--- |
| `off` | No scanning |
| `low` | Only high-confidence patterns (AWS keys, private keys) |
| `standard` | Default — all patterns including generic secrets |
| `strict` | Standard + extended patterns |
| `paranoid` | Everything, including heuristic detection |

### `dlp_custom_rules`

A list of custom DLP rules merged with built-in defaults.

| Field | Type | Description |
| :--- | :--- | :--- |
| `name` | string | Human-readable rule name |
| `pattern` | string | Regex or literal pattern to match |
| `action` | string | `redact`, `deny`, `audit`, or `allow` |

### `search`

Settings for the semantic search index.

| Field | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `persistence` | boolean | `false` | Persist the Tantivy index to disk at `.synapseed/index/` |
| `temporal_decay_lambda` | float | `0.01` | Decay rate for temporal search boost. Higher values = stronger recency preference. |

### `architect`

Settings for the architecture analysis engine.

| Field | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `density_high_threshold` | float | `0.5` | Topological density above this triggers a "high density" warning |
| `density_low_threshold` | float | `0.02` | Topological density below this (with ≥ `density_low_min_modules` modules) triggers a "low density" warning |
| `density_low_min_modules` | integer | `10` | Minimum module count before low-density warnings apply |

### `visualizer_port`

Port number for the live visualizer dashboard. Defaults to `3000`.

## Environment Variables

| Variable | Default | Description |
| :--- | :--- | :--- |
| `RUST_LOG` | `info` | Log level filter (trace/debug/info/warn/error) |
| `SYNAPSEED_LOG_FORMAT` | compact | Set to `json` for machine-readable logs |
| `SYNAPSEED_SELF_TELEMETRY` | (unset) | Set to `1` to enable self-instrumentation |
| `SYNAPSEED_VISUALIZER_PORT` | `3000` | Override the visualizer dashboard port |
