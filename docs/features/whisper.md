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
  → Generate smart_context summary (tier-adapted)
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

## Cognitive Tiers

Whisper adapts its output format based on the detected **Model Tier** — the cognitive capacity of the connected LLM.

| Tier | Target Models | Output Format |
| :--- | :--- | :--- |
| **Atomic** | <3B params (e.g., Qwen 2.5 0.5B) | Flat markdown, no `**bold**`, minimal structure |
| **Molecular** | 7B–32B (e.g., Codestral, Mistral) | Hybrid — structured sections with phase indicator |
| **Galactic** | Cloud/SOTA (e.g., Claude, GPT-4) | Dense output with SID metric and structured JSON |

### Tier Detection

Tiers are detected automatically via MCP client fingerprinting during `initialize`:

1. **DNA override** (always wins): Set `hci.model_profile: "atomic"` in `dna.yaml`
2. **Client fingerprint**: Extract `clientInfo.name` from the MCP `initialize` request
3. **Default**: `Molecular` if no information available

Known client fingerprints:

| Client Name | Detected Tier |
| :--- | :--- |
| `claude-code`, `claude`, `anthropic` | Galactic |
| `codex`, `openai`, `gpt` | Galactic |
| `gemini`, `google` | Galactic |
| `ollama`, `lmstudio`, `llamacpp` | Atomic |
| (everything else) | Molecular |

## Session Momentum

Whisper tracks tool invocations through the **Momentum Engine** to detect which phase of work the developer is in.

### Session Phases

| Phase | Triggered By | Behavior |
| :--- | :--- | :--- |
| **Discovery** | `hoist`, `lookup`, `search`, `similar`, `diagnose`, `consult`, `blame` | Broad context, exploration-focused output |
| **Implementation** | `quickfix`, `train`, `janitor`, `janitor-fix` | Focused, action-oriented output |
| **Stabilization** | `diagnostics`, `architect`, `scan`, `check`, `analyze` | Quality gate emphasis, risk-aware output |

The engine uses a sliding window (size 10) of recent tool invocations. When 3+ tools from the same phase category appear in the window, the phase transitions.

**Git-Context Alignment:** If `git diff --cached` detects staged files, the phase is forced to **Stabilization** regardless of the tool window — the developer is about to commit.

## Direct Symbol Injection (SID)

When `raw: true` is passed to the `ask` tool, Whisper injects the exact source code of discovered symbols into the response between delimiters:

```
--- FILE: src/main.rs (lines 10-25) ---
fn main() {
    // actual source code here
}
--- END ---
```

The **SID (Semantic Information Density)** metric measures how much useful code was injected relative to prompt size:

```
SID = symbols_found / (prompt_tokens / 1000)
```

Higher SID values indicate more information-dense responses.

## Response Format

The `ask` tool returns:

```json
{
  "smart_context": "Human-readable summary of findings",
  "intent": "bug_fix",
  "intent_scores": [["bug_fix", 5], ["explain", 2]],
  "diagnostics": [...],
  "histories": [
    {
      "file": "src/main.rs",
      "churn_score": 0.8,
      "convergence_rate": 0.95,
      "rigidity": 0.05,
      "fix_chain_count": 2,
      "co_changes": [...],
      "semantic_tags": [...]
    }
  ],
  "code_context": [...],
  "security_status": "CLEAN"
}
```

### Multi-Intent Classification

Whisper scores the query against all intent categories simultaneously, returning ranked `(intent, score)` pairs. The primary intent drives routing, but secondary intents influence context gathering — e.g., a "fix this security bug" query will engage both Bug/Fix and Security subsystems.

### Multi-File History

Instead of analyzing only one file, Whisper gathers git history for up to **5 unique files** across all discovered targets. This provides broader context about recent changes across the affected area of the codebase.

### Diagnostic Items in Context

When compiler diagnostics exist, the `smart_context` includes up to **10 actual error/warning messages** with severity, file path, line number, and message text — not just a count. This gives the LLM actionable information about build failures.

### Score-Aware Targets

Search result scores propagate through `Target.score`, enabling rank-aware deduplication (Sort First, Cut Later with `HashSet`) and context ordering. Targets from non-search sources (AST, diagnostics) carry `score: None` and sort by source priority.

### Enum & Constant Context Expansion

When injecting raw source code, the line range is automatically expanded for certain symbol kinds:

| Symbol Kind | Extra Lines | Rationale |
| :--- | :--- | :--- |
| `Enum` | +25 | Capture all enum variants |
| `Constant` | +15 | Capture full constant definitions |
| Others | 0 | Standard range |

This ensures the LLM sees the complete definition — not just the type header — for enums with many variants or grouped constant blocks.

### Score-Ordered Raw Source Injection

When injecting raw source code (Direct Symbol Injection), targets are sorted by composite search score **descending** before processing. This ensures the most relevant symbols always get budget priority. The injection pipeline uses three strategies:

1. **Score sorting** — targets with highest BM25+PageRank+Visibility scores are injected first
2. **Smart truncation** — oversized snippets are truncated (first 75% + last 25%) rather than skipped entirely
3. **Budget continuation** — when one target is too large, remaining targets still get a chance (no early exit)

For **Galactic** tier, the `smart_context` includes:
- Phase indicator (e.g., `[Phase: Stabilization]`)
- SID metric (e.g., `SID: 2.4 symbols/ktok`)
- Structured sections with bold headers

For **Atomic** tier, the `smart_context` is flat text with minimal formatting.

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

With Direct Symbol Injection:

```json
{
  "method": "tools/call",
  "params": {
    "name": "ask",
    "arguments": {
      "query": "explain the router module",
      "raw": true
    }
  }
}
```

This single call will:
1. Search for relevant symbols
2. Check compiler diagnostics
3. Analyze git history for recent changes
4. Scan for security issues
5. Adapt output to the detected model tier and session phase
6. Return a unified context object with SID metric (when `raw: true`)
