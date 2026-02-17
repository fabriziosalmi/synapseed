# Root — Command Sentinel

The **Root** module provides a policy-driven command execution sandbox. Every shell command suggested by the LLM is evaluated against the Sentinel before execution.

## Design Philosophy

**Fail-closed.** Commands that don't match any allow rule are DENIED by default. The sentinel never guesses — if it doesn't recognize a command as safe, it blocks it.

## How It Works

```
LLM suggests: "ls; rm -rf /"
  → Sentinel.evaluate("ls; rm -rf /")
  → Split on shell operators: ["ls", "rm -rf /"]
  → Evaluate each segment independently
  → "ls" → Allow (safe command)
  → "rm -rf /" → DENIED (destructive pattern)
  → Any deny → entire command DENIED
  → Return reason to LLM
```

## Policy Rules

The Sentinel uses regex-based rules organized as:

1. **Deny rules** (checked first) — Commands matching these are always blocked
2. **Allow rules** (checked second) — Commands matching these are permitted
3. **Default** — If no rule matches, the command is DENIED (fail-closed)

### Shell Chaining Defense (v4.29.0)

Commands containing shell operators (`;`, `|`, `&&`, `||`, newlines) are split into segments. Each segment is evaluated independently against the full rule set. If **any** segment is denied, the entire command is denied.

This prevents bypass attacks like `ls; rm -rf /` where a safe prefix would previously match the allow rule before the dangerous suffix was checked.

### Default Deny Rules

| Pattern | Blocks |
| :--- | :--- |
| `rm -rf /...` | Recursive delete from root |
| `mkfs`, `dd`, `fdisk`, `parted` | Disk operations |
| `chmod 777/0777/a+rwx/a=rwx` | World-writable permissions |
| `> /dev/sd*` | Raw device writes |
| `sudo` | Privilege escalation |
| `eval` | Shell eval |
| `curl ... \| sh` | Piped curl-to-shell |
| `LD_PRELOAD=` | Library injection |
| `$(...)`, `` `...` `` | Command substitution |
| `base64 -d/--decode` | Obfuscation via encoding |
| `python -c`, `ruby -e`, `perl -e`, `node -e` | Interpreter inline execution |
| `nohup` | Session escape |
| Null bytes | C-string truncation attacks |

### Example Evaluations

| Command | Result | Reason |
| :--- | :--- | :--- |
| `ls -la` | ALLOWED | Matches safe filesystem read |
| `cargo test` | ALLOWED | Matches safe build tool |
| `git status` | ALLOWED | Matches safe git operation |
| `ls && echo hello` | ALLOWED | Both segments match allow rules |
| `rm -rf /` | DENIED | Matches destructive pattern |
| `ls; rm -rf /` | DENIED | Chained: second segment denied |
| `chmod 0777 /etc/shadow` | DENIED | Matches permission escalation |
| `echo $(rm -rf /)` | DENIED | Command substitution blocked |
| `python -c 'os.system(...)'` | DENIED | Interpreter inline execution |
| `echo ... \| base64 -d` | DENIED | Obfuscation vector blocked |

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

synapseed check "ls; rm -rf /"
# DENIED: Chained command — segment "rm -rf /" denied
```
