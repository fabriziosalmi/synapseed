# MCP Resources

SYNAPSEED exposes 6 read-only resources. Resources provide context data that the LLM can inspect without side effects.

## `synapseed://status`

**Name:** Project Status

Current project state, runtime metrics, and plugin health.

```json
{
  "project_root": "/path/to/project",
  "state": "healthy_workspace",
  "metrics": {
    "files_indexed": 42,
    "symbols_found": 310,
    "dlp_scans": 5,
    "dlp_blocks": 0,
    "commands_allowed": 12,
    "commands_denied": 1,
    "errors_prevented": 1,
    "events_broadcast": 8
  }
}
```

---

## `synapseed://dna`

**Name:** Project DNA

Active configuration loaded from `.synapseed/dna.yaml`.

```json
{
  "workspace_strategy": "monorepo",
  "preferred_libs": { "async": "tokio", "json": "serde_json" },
  "naming": { "core_crate": "core", "bin_name": "synapseed" },
  "plugins": ["cortex", "husk", "root", "chronos"],
  "dlp_level": "Standard"
}
```

---

## `synapseed://security/policy`

**Name:** Security Policy

Active DLP rules and command execution policy with sample evaluations.

```json
{
  "dlp": {
    "engine": "aho-corasick + regex",
    "mode": "fail-closed"
  },
  "command_sentinel": {
    "mode": "fail-closed",
    "sample_evaluations": [
      { "command": "ls -la", "result": "ALLOWED" },
      { "command": "rm -rf /", "result": "DENIED" }
    ]
  }
}
```

---

## `synapseed://diagnostics/active`

**Name:** Active Diagnostics

Live compiler diagnostics from the background shadow compiler.

```json
{
  "error_count": 2,
  "warning_count": 5,
  "last_check_ms": 1234,
  "diagnostics": [...]
}
```

Returns `{"status": "inactive"}` if the shadow compiler is not running.

---

## `synapseed://visualizer/url`

**Name:** Visualizer Dashboard URL

URL and feature list for the live architecture dashboard.

```json
{
  "url": "http://localhost:3000",
  "description": "Live architecture dashboard",
  "features": [
    "Interactive code graph",
    "Real-time WebSocket updates",
    "Pulse animation on modifications",
    "Activity log"
  ]
}
```

---

## `synapseed://telemetry/hotspots`

**Name:** Telemetry Hotspots

Runtime performance hotspots from OTLP traces.

```json
{
  "total_spans": 150,
  "unique_locations": 12,
  "buffer_usage": "15%",
  "hotspots": [
    {
      "key": "src/auth.rs:verify_token",
      "call_count": 45,
      "avg_duration_ms": 250.5,
      "max_duration_ms": 890.0,
      "p95_duration_ms": 450.0
    }
  ]
}
```

Returns `{"status": "inactive"}` if the telemetry sink is not running.
