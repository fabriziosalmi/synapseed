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

# DLP false-positive whitelist (regex patterns)
dlp_whitelist:
  - CancellationToken
  - test_secret_\w+

# Search index settings
search:
  persistence: true
  embeddings: true
  temporal_decay_lambda: 0.01

# Context configuration (Whisper symbol pruning)
context:
  max_symbols: 5

# HCI (Human-Computer Interaction) settings
hci:
  background_indexing: true
  port_retry: true
  adaptive_linting: true
  mentor_mode: true
  session_persistence: true
  memory_ceiling_files: 10000
  model_profile: null  # "atomic", "molecular", or "galactic"

# Code security pattern scanning
security_patterns:
  enabled: true
  categories: []  # empty = all active

# Architect settings
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

### `dlp_whitelist`

A list of regex patterns to suppress false-positive DLP findings. If a finding's matched text contains any whitelist pattern, it is ignored.

Example: `"CancellationToken"` suppresses `generic_secret` hits on Rust concurrency types.

### `search`

Settings for the semantic search index.

| Field | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `persistence` | boolean | `false` | Persist the Tantivy index to disk at `.synapseed/index/` |
| `embeddings` | boolean | `false` | Enable vector embedding similarity search. Downloads ~22MB model on first use to `.synapseed/models/`. Required for the `similar` tool. |
| `temporal_decay_lambda` | float | `0.01` | Decay rate for temporal search boost. Higher values = stronger recency preference. Score formula: `bm25 × (0.7 + 0.3 × e^(−λ × age_days))`. |

### `context`

Controls how many symbols the Whisper intent router includes in responses.

| Field | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `max_symbols` | integer | `5` | Maximum symbols to include in context. Lower values reduce noise for ultra-small models (<3B). |

### `hci`

Human-Computer Interaction settings. Controls perceptual quality features.

| Field | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `background_indexing` | boolean | `true` | Enable non-blocking code indexing at startup |
| `port_retry` | boolean | `true` | Enable automatic port retry for the telemetry gRPC server |
| `adaptive_linting` | boolean | `true` | Enable debounce escalation during rapid edits |
| `mentor_mode` | boolean | `true` | Response depth adapts to query complexity |
| `session_persistence` | boolean | `true` | Enable cross-session continuity |
| `memory_ceiling_files` | integer | `null` | Max files to index (memory ceiling). `null` = unlimited (default: 10000) |
| `model_profile` | string | `null` | Cognitive profile override: `"atomic"`, `"molecular"`, or `"galactic"`. Overrides auto-detected tier from MCP client fingerprinting. |

### `security_patterns`

Controls which vulnerability categories are checked by the `scan` tool's code pattern scanner.

| Field | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `enabled` | boolean | `true` | Enable code pattern scanning |
| `categories` | list | `[]` | Active categories: `sql_injection`, `xss`, `command_injection`, `path_traversal`. Empty = all active. |

### `architect`

Settings for the architecture analysis engine.

| Field | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `density_high_threshold` | float | `0.5` | Topological density above this triggers a "high density" warning |
| `density_low_threshold` | float | `0.02` | Topological density below this (with ≥ `density_low_min_modules` modules) triggers a "low density" warning |
| `density_low_min_modules` | integer | `10` | Minimum module count before low-density warnings apply |
| `god_object_max_symbols` | integer | `50` | Max public symbols before flagging as god object |
| `god_object_max_lines` | integer | `1000` | Max approximate lines before flagging as god object |
| `god_object_min_fan_in` | integer | `5` | Min fan-in to combine with size for god object detection |
| `layers` | list | `[]` | Layer definitions for violation detection. Each entry has `name`, `rank`, and `modules`. |

## Environment Variables

| Variable | Default | Description |
| :--- | :--- | :--- |
| `RUST_LOG` | `info` | Log level filter (trace/debug/info/warn/error) |
| `SYNAPSEED_LOG_FORMAT` | compact | Set to `json` for machine-readable logs |
| `SYNAPSEED_SELF_TELEMETRY` | (unset) | Set to `1` to enable self-instrumentation |
