# Shadow — Live Compiler

The **Shadow Compiler** runs `cargo check` in the background and provides live diagnostics to the LLM. It enables the LLM to check compilation status and auto-fix issues without manual intervention.

## How It Works

```
Project root detected with Cargo.toml
  → ShadowCheckPlugin spawns background thread
  → Runs: cargo check --message-format=json
  → Parses JSON output into structured diagnostics
  → Stores in DiagnosticStore (thread-safe)
  → Broadcasts DiagnosticUpdated event
  → Re-checks on file changes (debounced)
```

## Diagnostic Types

Each diagnostic includes:

- **File path** — Source file containing the error
- **Line/column** — Exact location
- **Severity** — Error or Warning
- **Code** — Rust error code (e.g., `E0425`, `unused_variables`)
- **Message** — Human-readable description
- **Suggestions** — Compiler-suggested fixes with applicability level

## Suggestion Applicability

| Level | Meaning | Auto-apply? |
| :--- | :--- | :--- |
| `MachineApplicable` | Safe to apply automatically | Yes |
| `MaybeIncorrect` | Might not be right | Ask user first |
| `HasPlaceholders` | Requires manual input | No |
| `Unspecified` | Unknown | No |

## MCP Integration

| Tool | Description |
| :--- | :--- |
| `get_diagnostics` | Get current errors/warnings, optionally filtered by file |
| `apply_quick_fix` | Apply a `MachineApplicable` fix by file and error code |

## Usage Flow

1. LLM writes code
2. Shadow compiler detects change, re-checks
3. LLM calls `get_diagnostics` to see errors
4. LLM calls `apply_quick_fix` for auto-fixable issues
5. Repeat until clean

```json
// Step 1: Check for errors
{"method": "tools/call", "params": {"name": "get_diagnostics", "arguments": {}}}

// Step 2: Fix an error
{"method": "tools/call", "params": {"name": "apply_quick_fix", "arguments": {"file": "src/main.rs", "error_code": "unused_variables"}}}
```
