# Husk — DLP Shield

The **Husk** is SYNAPSEED's security shield. It scans every piece of content for sensitive data and blocks operations on detection.

## Design Philosophy

**Fail-closed.** If a secret is found, the operation is blocked and the content is redacted. No exceptions, no overrides through the LLM.

## What It Detects

| Pattern | Example | Method |
| :--- | :--- | :--- |
| AWS Access Keys | `AKIAIOSFODNN7EXAMPLE` | Aho-Corasick prefix + length validation |
| GitHub Tokens | `ghp_xxxx`, `github_pat_xxxx` | Aho-Corasick prefix matching |
| Generic Secrets | `password=hunter2`, `api_key=xxx` | Regex pattern matching |
| Private Keys | `-----BEGIN RSA PRIVATE KEY-----` | Aho-Corasick marker detection |

## How It Works

```
Input content
  → Aho-Corasick multi-pattern scan (O(n) in content length)
  → Regex structured pattern scan
  → If any match:
      → BLOCK operation
      → REDACT sensitive portions
      → Return findings with severity
  → If clean: PASS
```

## Technology

- **Aho-Corasick** (v1) — Simultaneous multi-pattern matching in a single pass. Builds a finite automaton from all patterns, scans content in O(n) time regardless of pattern count.
- **Regex** (v1) — For structured patterns that require context (e.g., `password=<value>`).

## MCP Integration

| Tool | Description |
| :--- | :--- |
| `scan_security` | Scan text content. Supports `mode`: `all` (DLP + patterns), `dlp` (secrets only), `patterns` (code vulnerability patterns only) |

## Code Pattern Scanner

In addition to DLP secret detection, Husk includes a `CodePatternScanner` for static vulnerability detection. It uses regex-based heuristics to identify common security anti-patterns:

| Category | Patterns | Examples |
| :--- | :--- | :--- |
| SQL Injection | 3 patterns | `format!("SELECT ... {}")`, string concatenation in queries |
| XSS | 4 patterns | `innerHTML =`, `document.write()`, `v-html`, `dangerouslySetInnerHTML` |
| Command Injection | 4 patterns | `Command::new()` with user input, `exec()`, `eval()` |
| Path Traversal | 3 patterns | `../` sequences, unsanitized path joins |

Each finding includes the category, line number, risk level, confidence score, and a remediation suggestion.

### Usage

```json
// Scan for code patterns only
{"method": "tools/call", "params": {"name": "scan_security", "arguments": {"content": "...", "mode": "patterns"}}}

// Scan for everything (DLP + patterns)
{"method": "tools/call", "params": {"name": "scan_security", "arguments": {"content": "...", "mode": "all"}}}
```

## Usage Example

```bash
# Clean content
synapseed scan --text "perfectly safe text"
# CLEAN: No sensitive data detected.

# AWS key detected
synapseed scan --text "aws_key=AKIAIOSFODNN7EXAMPLE"
# ALERT: AWS Access Key detected
# Sanitized: aws_key=REDACTED
```

## DLP Levels

Configured via `dlp_level` in `dna.yaml`:

| Level | Behavior |
| :--- | :--- |
| `off` | No scanning |
| `low` | Only high-confidence patterns |
| `standard` | All patterns (default) |
| `strict` | Standard + extended heuristics |
| `paranoid` | Maximum sensitivity |
