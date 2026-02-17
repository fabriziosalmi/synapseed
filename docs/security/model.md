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

- **Shell chaining defense** — commands split on `;`, `|`, `&&`, `||`, newlines; each segment evaluated independently
- **Null byte rejection** — blocks C-string truncation attacks
- Deny patterns checked first (destructive commands, command substitution, obfuscation vectors)
- Allow patterns checked second (safe commands)
- Default: DENIED

**Deny coverage:** `rm -rf`, `mkfs`/`dd`, `chmod 777`/`0777`/`a+rwx`, `sudo`, `eval`, `curl|sh`, `LD_PRELOAD`, `$()`, backticks, `base64 -d`, interpreter inline execution (`python -c`, `ruby -e`, `perl -e`, `node -e`), `nohup`.

**Engine:** Regex-based rules with shell operator splitting.
**Mode:** Fail-closed. Unknown commands are blocked.

### Layer 3: Network Isolation

- Telemetry Sink binds only to `127.0.0.1:4317` (localhost)
- No outbound network calls from any subsystem
- Self-telemetry sends only to localhost

### Layer 4: Process Boundary

- No arbitrary subprocess spawning
- Only controlled `cargo check` via Shadow Compiler
  > **Note:** `cargo check` executes `build.rs` scripts with full user privileges — not sandboxed. Disable with `hci.shadow_check: false` in `dna.yaml` for untrusted projects. See [Build Script Security](build-scripts.md).
- No file writes except `quickfix` (compiler-suggested only)
- No environment variable modification

## Threat Model

| Threat | Mitigation |
| :--- | :--- |
| LLM leaks secrets via tool response | DLP scans all content (Husk) |
| LLM suggests destructive command | Sentinel evaluates all commands with chaining defense (Root) |
| LLM chains safe + destructive commands | Shell operator splitting evaluates each segment independently |
| Obfuscated command (base64, interpreter) | Deny rules for encoding and inline execution vectors |
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
