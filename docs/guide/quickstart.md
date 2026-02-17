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

## Start the MCP Server

```bash
synapseed serve --project .
```

This starts the JSON-RPC 2.0 server on stdin/stdout, ready for Claude Desktop, VS Code extension, or any MCP-compatible client.

## VS Code Integration

Install the [VS Code extension](/integration/vscode-extension) for real-time panels showing project status, diagnostics, architecture health, and more.

```bash
cd vscode-extension
npm install && npm run package
code --install-extension synapseed-0.3.0.vsix
```

## 8. Ask a Question (Recommended)

For the best experience, use the intent router which orchestrates all subsystems:

```bash
synapseed ask "why is authentication failing?"
# Returns orchestrated response using search, git history, diagnostics, etc.

synapseed ask "what changed in the last week?"
# Automatically uses git history and semantic analysis
```

## 9. Check Architecture Health

```bash
synapseed architect --refresh
# Returns: Grade A, 97/100, 0 violations, 131 modules
```

## 10. Run Maintenance Scan

```bash
synapseed janitor
# Scans for clippy warnings and unused dependencies
# Returns: Actionable fix proposals with UUIDs
```

## Self-Telemetry (Dogfooding)

To enable SYNAPSEED to observe its own performance:

```bash
SYNAPSEED_SELF_TELEMETRY=1 synapseed serve --project .
```

This sends internal tracing spans to the built-in OTLP receiver, enabling heatmap visualization of SYNAPSEED's own hotspots.
