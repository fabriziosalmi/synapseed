# Whisper — Intent Router

The **Whisper** module is SYNAPSEED's Intent Router. It takes a natural-language question and automatically orchestrates all relevant subsystems in a single call, returning a rich context object.

## Why?

Without Whisper, the LLM must make multiple sequential MCP tool calls:
1. `hoist` to understand structure
2. `lookup` to find relevant code
3. `blame` to understand context
4. `diagnostics` to check build status
5. `scan` to verify safety

**Whisper does all of this in one call**, reducing roundtrips and context window usage.

## How It Works

```
Natural language query
  → Intent classifier (keyword heuristics)
  → Route to subsystems based on intent
  → Execute all relevant tools in parallel
  → Aggregate results into EnrichedContext
  → Generate smart_context summary
  → Return to LLM
```

## Intent Categories

| Intent | Keywords | Subsystems Invoked |
| :--- | :--- | :--- |
| Bug/Fix | fix, bug, error, broken, crash | Diagnostics, History, Code context |
| Security | security, audit, secret, vulnerability | DLP scan, Sentinel check, History |
| Explain | explain, understand, how, why, what | Code skeleton, History, Search |
| Refactor | refactor, cleanup, rename, improve | Code skeleton, History, Diagnostics |
| General | (everything else) | Code skeleton, Search |

## Response Format

The `ask` tool returns:

```json
{
  "smart_context": "Human-readable summary of findings",
  "intent": "bug_fix",
  "diagnostics": [...],
  "history": {
    "churn_score": 0.8,
    "convergence_rate": 0.95,
    "rigidity": 0.05,
    "fix_chain_count": 2,
    "co_changes": [...],
    "semantic_tags": [...]
  },
  "code_context": [...],
  "security_status": "CLEAN"
}
```

## MCP Integration

| Tool | Description |
| :--- | :--- |
| `ask` | Ask a natural-language question, get orchestrated response |

## Usage Example

```json
{
  "method": "tools/call",
  "params": {
    "name": "ask",
    "arguments": {
      "query": "why is the login broken?"
    }
  }
}
```

This single call will:
1. Search for login-related symbols
2. Check compiler diagnostics
3. Analyze git history for recent changes
4. Scan for security issues
5. Return a unified context object
