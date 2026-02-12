# Root — Command Sentinel

The **Root** module provides a policy-driven command execution sandbox. Every shell command suggested by the LLM is evaluated against the Sentinel before execution.

## Design Philosophy

**Fail-closed.** Commands that don't match any allow rule are DENIED by default. The sentinel never guesses — if it doesn't recognize a command as safe, it blocks it.

## How It Works

```
LLM suggests: "rm -rf /"
  → Sentinel.evaluate("rm -rf /")
  → Check against deny patterns (destructive commands)
  → Check against allow patterns (safe commands)
  → No allow match → DENIED
  → Return reason to LLM
```

## Policy Rules

The Sentinel uses regex-based rules organized as:

1. **Deny rules** (checked first) — Commands matching these are always blocked
2. **Allow rules** (checked second) — Commands matching these are permitted
3. **Default** — If no rule matches, the command is DENIED (fail-closed)

### Example Evaluations

| Command | Result | Reason |
| :--- | :--- | :--- |
| `ls -la` | ALLOWED | Matches safe filesystem read |
| `cargo test` | ALLOWED | Matches safe build tool |
| `git status` | ALLOWED | Matches safe git operation |
| `rm -rf /` | DENIED | Matches destructive pattern |
| `chmod 777 /etc/shadow` | DENIED | Matches permission escalation |
| `dd if=/dev/zero of=/dev/sda` | DENIED | Matches disk write pattern |
| `curl https://example.com` | ALLOWED | Matches safe network tool |

## MCP Integration

| Tool | Description |
| :--- | :--- |
| `check` | Evaluate a command and return ALLOWED or DENIED with reason |

## Usage Example

```bash
synapseed check "cargo build"
# ALLOWED (Safe): cargo build

synapseed check "rm -rf /"
# DENIED: Matches destructive pattern
```
