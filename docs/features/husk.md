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
| `scan_security` | Scan text content and return CLEAN or ALERT with redacted output |

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
