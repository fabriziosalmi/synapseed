# Search — Semantic Index

The **Search** module provides concept-based code search powered by **Tantivy**, a Rust full-text search engine. Instead of exact string matching, it finds code related to ideas.

## Why Not Grep?

| grep | SYNAPSEED Search |
| :--- | :--- |
| Exact string matching | Concept-based matching |
| `grep "auth"` misses `verify_credentials` | `search "authentication"` finds both |
| No ranking | TF-IDF relevance scoring |
| No fuzzy matching | `auth~2` handles typos |

## What Gets Indexed

For each symbol in the codebase:

| Field | Content | Boost |
| :--- | :--- | :--- |
| `name` | Symbol name | High |
| `signature` | Function/method signature | Medium |
| `doc_comment` | Documentation comments | Medium |
| `body` | First 500 chars of body | Low |
| `file_path` | Source file path | Low |
| `kind` | Symbol kind (function, struct, etc.) | — |
| `line_start` / `line_end` | Source location | — |

## How It Works

```
Cortex indexes project → AST symbols
  → Search builds Tantivy index (in-memory or persistent)
  → Query parsed with Tantivy query parser
  → TF-IDF scoring + fuzzy matching
  → Results ranked by relevance
```

## Disk Persistence

By default the search index is built in-memory and rebuilt on each startup. To persist the index to disk, enable persistence in your `dna.yaml`:

```yaml
search:
  persistence: true
```

When enabled, the Tantivy index is written to `.synapseed/index/` and reused across restarts. The index is incrementally updated when files change, which significantly speeds up startup for large projects.

## MCP Integration

| Tool | Description |
| :--- | :--- |
| `semantic_search` | Search for code by concept. Supports fuzzy matching. |

## Query Syntax

| Query | Meaning |
| :--- | :--- |
| `authentication login` | Find symbols related to both terms |
| `auth~2` | Fuzzy match with edit distance 2 |
| `"error handling"` | Exact phrase match |
| `kind:function name:parse` | Field-specific search |

## Usage Example

```json
{
  "method": "tools/call",
  "params": {
    "name": "semantic_search",
    "arguments": {
      "query": "authentication login",
      "limit": 5
    }
  }
}
```
