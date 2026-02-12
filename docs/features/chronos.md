# Chronos — Git Time-Travel

**Chronos** provides deep git intelligence, enabling the LLM to understand not just *what* the code does, but *why* it was written that way.

## Capabilities

- **Blame intelligence** — Who changed what line and when
- **Commit history** — Full log with filtering by file and line range
- **Churn analysis** — Identifies volatile files and hotspot regions
- **Co-change detection** — Finds files that always change together
- **Semantic classification** — Tags commits as fix, feature, refactor, revert, security
- **Risk assessment** — Combines churn, fix frequency, and revert history

## How It Works

Chronos uses **libgit2** (via the `git2` crate) for direct repository access — no subprocess calls, no `git` binary dependency.

```
Git Repository (.git/)
  → libgit2 bindings
  → Blame analysis (line-level attribution)
  → Commit walker (filtered by path/line range)
  → Semantic tagger (regex on commit messages)
  → Churn calculator (edit frequency over time)
  → Co-change detector (files in same commits)
```

## Semantic Commit Tags

| Tag | Pattern | Example |
| :--- | :--- | :--- |
| `FIX` | fix, bug, patch, hotfix, resolve | "Fix null pointer in auth" |
| `REVERT` | revert, rollback, undo | "Revert broken migration" |
| `REFACTOR` | refactor, cleanup, rename, restructure | "Refactor handler logic" |
| `SECURITY` | security, vulnerability, CVE, XSS, injection | "Fix SQL injection in query" |
| `FEATURE` | feat, add, implement, introduce, new | "Add OAuth2 support" |

## MCP Integration

| Tool | Description |
| :--- | :--- |
| `git_history` | Get blame data for a file and line range |
| `analyze_history` | Full file analysis: churn, co-changes, risk, semantic tags |

## Usage Example

```bash
# Blame a file region
synapseed blame src/auth.rs --start 10 --end 30

# View recent commits
synapseed history --project . --limit 5
```
