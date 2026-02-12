# Whisper — Intent Router

The **Whisper** module is SYNAPSEED's Intent Router. It takes a natural-language question and automatically orchestrates all relevant subsystems in a single call, returning a rich context object.

## Why?

Without Whisper, the LLM must make multiple sequential MCP tool calls:
1. `get_code_skeleton` to understand structure
2. `lookup_symbol` to find relevant code
3. `git_history` to understand context
4. `get_diagnostics` to check build status
5. `scan_security` to verify safety

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

The `ask_whisperer` tool returns:

```json
{
  "smart_context": "Human-readable summary of findings",
  "intent": "bug_fix",
  "diagnostics": [...],
  "history": {...},
  "code_context": [...],
  "security_status": "CLEAN"
}
```

## MCP Integration

| Tool | Description |
| :--- | :--- |
| `ask_whisperer` | Ask a natural-language question, get orchestrated response |

## Usage Example

```json
{
  "method": "tools/call",
  "params": {
    "name": "ask_whisperer",
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
