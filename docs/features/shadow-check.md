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
| `diagnostics` | Get current errors/warnings, optionally filtered by file |
| `quickfix` | Apply a `MachineApplicable` fix by file and error code |

## Usage Flow

1. LLM writes code
2. Shadow compiler detects change, re-checks
3. LLM calls `diagnostics` to see errors
4. LLM calls `quickfix` for auto-fixable issues
5. Repeat until clean

```json
// Step 1: Check for errors
{"method": "tools/call", "params": {"name": "diagnostics", "arguments": {}}}

// Step 2: Fix an error
{"method": "tools/call", "params": {"name": "quickfix", "arguments": {"file": "src/main.rs", "error_code": "unused_variables"}}}
```

## Security Considerations (D39)

> **Warning:** The Shadow Compiler executes `cargo check` as a child process
> with the **full privileges of the current user**. Cargo's check phase
> compiles and **runs `build.rs` build scripts**, which can contain arbitrary
> code (network I/O, file system writes, environment variable reads, etc.).

### Risks

| Risk | Description |
| :--- | :--- |
| **Malicious `build.rs`** | An untrusted project can execute arbitrary code when the Shadow Compiler runs `cargo check` in the background. |
| **Supply chain** | A compromised dependency's `build.rs` runs automatically. |
| **Data exfiltration** | A `build.rs` can read `~/.ssh`, `~/.aws`, environment variables, and send them over the network. |

### Mitigations

1. **Only open trusted projects.** The same risk applies to any `cargo build` / `cargo check` invocation — this is a general Cargo security property, not specific to Synapseed.
2. **Review `build.rs` files** in new projects and dependencies before opening them.
3. **DNA opt-out:** Disable background compilation in `.synapseed/dna.yaml`:
   ```yaml
   hci:
     shadow_check: false
   ```
4. **Environment hardening:** Never run in shells with production credentials in environment variables.
5. **Future work:** Sandbox `cargo check` via `bubblewrap` (Linux) or similar isolation on supported platforms.

For the full threat model and review checklist, see [Build Script Security](../security/build-scripts.md).
