# Security Model

SYNAPSEED enforces a **defense-in-depth** security model with multiple layers of protection.

## Principles

1. **Fail-closed** — When in doubt, block. No default-allow anywhere.
2. **Local-first** — No cloud, no network calls, no telemetry leaks.
3. **Zero-trust for LLMs** — The LLM cannot bypass security checks.
4. **Audit everything** — All blocks and alerts are logged.

## Security Layers

### Layer 1: DLP Shield + Code Pattern Scanner (Husk)

Every piece of content leaving SYNAPSEED is scanned for sensitive data:

- API keys (AWS, GitHub, generic)
- Passwords and credentials
- Private key material
- PII patterns

Additionally, the `CodePatternScanner` detects 14 common vulnerability patterns across 4 categories: SQL injection, XSS, command injection, and path traversal.

**Engine:** Aho-Corasick multi-pattern matching + regex + static pattern analysis.
**Mode:** Fail-closed. Any finding blocks the operation.

### Layer 2: Command Sentinel (Root)

Every shell command suggested by the LLM is evaluated:

- Deny patterns checked first (destructive commands)
- Allow patterns checked second (safe commands)
- Default: DENIED

**Engine:** Regex-based whitelist/blacklist.
**Mode:** Fail-closed. Unknown commands are blocked.

### Layer 3: Network Isolation

- Visualizer binds only to `127.0.0.1:3000` (localhost)
- Telemetry Sink binds only to `127.0.0.1:4317` (localhost)
- No outbound network calls from any subsystem
- Self-telemetry sends only to localhost

### Layer 4: Process Boundary

- No arbitrary subprocess spawning
- Only controlled `cargo check` via Shadow Compiler
- No file writes except `apply_quick_fix` (compiler-suggested only)
- No environment variable modification

## Threat Model

| Threat | Mitigation |
| :--- | :--- |
| LLM leaks secrets via tool response | DLP scans all content (Husk) |
| LLM suggests destructive command | Sentinel evaluates all commands (Root) |
| LLM accesses sensitive files | Read-only AST analysis, no raw file content exposure |
| Network exfiltration | All servers bind to localhost only |
| Supply chain attack via dependencies | Minimal dependency tree, Cargo audit |
| Self-telemetry data leakage | Localhost-only OTLP, no external endpoints |

## Audit Trail

All security events are logged:

```
[WARN] DLP: AWS Access Key detected in scan
[INFO] Sentinel: DENIED "rm -rf /" — matches destructive pattern
[INFO] Sentinel: ALLOWED "cargo test" — matches safe build tool
```

In MCP serve mode, all logs go to stderr to avoid corrupting the JSON-RPC transport.
