# MCP Resources

SYNAPSEED exposes 13 read-only resources. Resources provide context data that the LLM can inspect without side effects.

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

---

## `synapseed://janitor/proposals`

**Name:** Janitor Proposals

Active fix proposals from the most recent Janitor scan.

```json
{
  "proposals": [
    {
      "id": "a1b2c3d4-...",
      "kind": "clippy",
      "file": "src/lib.rs",
      "description": "unused variable `x`",
      "risk": "low"
    }
  ],
  "scan_in_progress": false
}
```

Returns `{"status": "no_proposals"}` if the Janitor hasn't run yet.

---

## `synapseed://architect/health`

**Name:** Architecture Health

Structural health snapshot from the most recent architect analysis.

```json
{
  "score": 97,
  "grade": "A",
  "modules": 131,
  "edges": 44,
  "topological_density": 0.0026,
  "violations": 1,
  "max_coupling": 2,
  "avg_instability": 0.16
}
```

Returns `{"status": "not_analyzed"}` if `architect` hasn't been called yet.

---

## `synapseed://consistency`

**Name:** Consistency Report

Cross-references workspace members, README, docs, and crate metadata for inconsistencies.

```json
{
  "total_checks": 4,
  "inconsistencies": [
    {
      "category": "workspace_members",
      "severity": "warning",
      "description": "Crate 'foo' exists on disk but is not listed in workspace members",
      "suggestion": "Add 'crates/foo' to workspace members in Cargo.toml"
    }
  ],
  "score": 0.92
}
```

The `score` is `1.0` when no inconsistencies are found.

---

## `synapseed://session/recorder`

**Name:** Session Flight Recorder

Session memory with working set (recently accessed files/symbols) and journey map (development phases).

```json
{
  "working_set": ["src/auth.rs", "src/config.rs"],
  "journey": [
    { "phase": "exploration", "modules": ["auth", "config"], "duration_secs": 120 }
  ]
}
```

---

## `synapseed://session/context`

**Name:** Cognitive Ledger — Session Pulse

Deterministic Operational Moment classification and session pulse data.

```json
{
  "operational_moment": "deep_work",
  "confidence": 0.85,
  "session_duration_secs": 600
}
```

---

## `synapseed://context/active`

**Name:** Active Context Briefing

**PRIORITY RESOURCE** — Dynamic project briefing that aggregates key signals for immediate situational awareness. Preloading this resource eliminates multiple initial tool calls.

```json
{
  "project_state": "healthy_workspace",
  "state_detail": { "build_system": "Cargo", "file_count": 42 },
  "dna": {
    "strategy": "monorepo",
    "dlp_level": "Standard",
    "plugins": ["cortex", "husk", "root", "chronos"]
  },
  "diagnostics": { "errors": 0, "warnings": 3 },
  "architecture_grade": "B",
  "metrics": { "files_indexed": 42, "symbols_found": 310 },
  "session": { "time_ago": "5 minutes ago", "files_indexed": 42, "tools_invoked": 8 },
  "routing_hint": "For ANY code question, call the `ask` tool FIRST."
}
```

This resource supports the **Passive Interception** strategy: clients that preload it give the LLM project context before any tool call, reducing roundtrips.

---

## `synapseed://pulse`

**Name:** Activity Pulse

Exponential-decay working set tracking files and symbols with recency-weighted activity scores.

```json
{
  "files": [
    { "path": "src/auth.rs", "score": 0.95, "last_seen_secs_ago": 30 }
  ],
  "symbols": [
    { "name": "verify_token", "score": 0.88, "last_seen_secs_ago": 45 }
  ]
}
```

---

## `synapseed://pipeline/metrics`

**Name:** Pipeline Performance Metrics

Ingest throughput and enrichment statistics for the indexing pipeline.

```json
{
  "files_processed": 42,
  "symbols_indexed": 310,
  "enrichment_passes": 3,
  "pipeline_duration_ms": 1250
}
```
