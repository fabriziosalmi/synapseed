# Quick Start

## 1. Index Your Project

```bash
synapseed hoist --project /path/to/your/project
```

This parses all source files (Rust, Python, JavaScript) and outputs a JSON summary of the AST skeleton.

## 2. Find a Symbol

```bash
synapseed lookup --project . MyStruct
```

Returns the file path, line numbers, kind, and signature for every symbol matching `MyStruct`.

## 3. Scan for Secrets

```bash
synapseed scan --text "aws_key=AKIAIOSFODNN7EXAMPLE"
# ALERT: AWS Access Key detected
# Sanitized: aws_key=REDACTED
```

## 4. Check a Command

```bash
synapseed check --project . "cargo test"
# ALLOWED (Safe): cargo test

synapseed check --project . "rm -rf /"
# DENIED: Matches destructive pattern
```

## 5. View Git History

```bash
synapseed history --project . --limit 5
```

## 6. Run Full Diagnostic

```bash
synapseed diagnose --project .
```

Outputs project state, DNA configuration, git status, and all metrics.

## 7. Start the MCP Server

```bash
synapseed serve --project .
```

This starts the JSON-RPC 2.0 server on stdin/stdout, ready for Claude Desktop or any MCP-compatible client.

## 8. Open the Visualizer

When running in `serve` mode, open your browser at:

```
http://localhost:3000
```

You'll see an interactive graph of your codebase with live WebSocket updates.

## Self-Telemetry (Dogfooding)

To enable SYNAPSEED to observe its own performance:

```bash
SYNAPSEED_SELF_TELEMETRY=1 synapseed serve --project .
```

This sends internal tracing spans to the built-in OTLP receiver, enabling heatmap visualization of SYNAPSEED's own hotspots.
