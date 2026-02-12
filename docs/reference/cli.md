# CLI Commands

SYNAPSEED provides a CLI for standalone analysis and MCP server operation.

## Global Options

```bash
synapseed [OPTIONS] <COMMAND>

Options:
  -p, --project <PATH>   Project root directory [default: .]
  -h, --help             Print help
  -V, --version          Print version
```

## Commands

### `hoist`

Index a project and display its code graph skeleton.

```bash
synapseed hoist --project /path/to/project
```

Output: JSON with `files_indexed` and `symbols_indexed` counts.

---

### `lookup <NAME>`

Look up a symbol by name across the indexed project.

```bash
synapseed lookup --project . MyStruct
```

Output: JSON array of matching symbols with file path, line numbers, kind, and signature.

---

### `scan`

Scan content for sensitive data (DLP check).

```bash
# From argument
synapseed scan --text "api_key=sk-live-abc123"

# From stdin
echo "secret_token=ghp_xxxx" | synapseed scan
```

Output: `CLEAN` or `ALERT` with redacted content.

---

### `check <COMMAND>`

Evaluate a shell command against the security sentinel.

```bash
synapseed check "cargo test"
# ALLOWED (Safe): cargo test

synapseed check "rm -rf /"
# DENIED: Matches destructive pattern
```

---

### `diagnose`

Run full system diagnostic.

```bash
synapseed diagnose --project .
```

Output: Project state, DNA configuration, git status, and metrics.

---

### `history`

Show git history summary and recent commits.

```bash
synapseed history --project . --limit 10
```

Options:
- `--limit, -l <N>` — Number of recent commits (default: 10)

---

### `blame <FILE>`

Show blame information for a file region.

```bash
synapseed blame --project . src/main.rs --start 1 --end 20
```

Options:
- `--start, -s <LINE>` — Start line (default: 1)
- `--end, -e <LINE>` — End line (default: 20)

---

### `status`

Show runtime metrics and system status.

```bash
synapseed status --project .
```

Initializes core plugins and displays all metrics.

---

### `init`

Initialize all plugins and broadcast SystemInit event.

```bash
synapseed init --project .
```

Useful for testing plugin initialization and event flow.

---

### `serve`

Start MCP server (JSON-RPC 2.0 over stdio).

```bash
synapseed serve --project .
```

This is the primary mode for LLM integration. All output goes to stderr; stdout is reserved for JSON-RPC.

#### Environment Variables

| Variable | Description |
| :--- | :--- |
| `RUST_LOG` | Log level (default: `info`) |
| `SYNAPSEED_SELF_TELEMETRY` | Set to `1` for self-instrumentation |
