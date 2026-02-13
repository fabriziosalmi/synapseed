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
| `last_modified_epoch` | File modification timestamp (u64) | — |

## How It Works

```
Cortex indexes project → AST symbols
  → Search builds Tantivy index (in-memory or persistent)
  → Query parsed with Tantivy query parser
  → Three-tier search cascade: BM25 → Prefix → Fuzzy
  → Additive normalized scoring (8 features, weights sum to 1.0)
  → Results ranked by composite relevance
```

## Search Cascade

Search uses a progressive three-tier cascade to maximize recall without sacrificing precision:

1. **BM25** — Standard full-text search with the Tantivy query parser
2. **Prefix matching** — Falls back to `RegexQuery` (`query.*`) on `symbol_name` when BM25 returns insufficient results. Catches partial matches like `handle_req` → `handle_request`
3. **Fuzzy matching** — Final fallback with Levenshtein distance for typo tolerance

Each tier only activates if the previous tier didn't return enough results.

## Scoring Model

Search results are scored using an **additive normalized model** with 8 weighted features, all min-max normalized to [0, 1]:

| Feature | Weight | Description |
| :--- | :--- | :--- |
| BM25 | 0.45 | Tantivy TF-IDF relevance |
| Source | 0.15 | Priority by extraction source |
| Path | 0.10 | File path proximity boost |
| PageRank | 0.10 | Module authority (symbol graph) |
| Visibility | 0.05 | Public API prioritization |
| Kind | 0.05 | Symbol kind preference |
| Specificity | 0.05 | Name specificity boost |
| Temporal | 0.05 | File recency decay |

```
score = W_BM25 × norm(bm25) + W_SOURCE × norm(source) + ... + W_TEMPORAL × norm(temporal)
```

## Temporal Boost

Search results are weighted by file recency. Recently modified files score higher than stale ones. The temporal boost formula is:

```
adjusted_score = raw_score × (0.7 + 0.3 × e^(−λ × age_days))
```

Where `λ` defaults to `0.01` and is configurable via `search.temporal_decay_lambda` in `dna.yaml`:

```yaml
search:
  temporal_decay_lambda: 0.02  # faster decay = stronger recency preference
```

## Disk Persistence

By default the search index is built in-memory and rebuilt on each startup. To persist the index to disk, enable persistence in your `dna.yaml`:

```yaml
search:
  persistence: true
```

When enabled, the Tantivy index is written to `.synapseed/index/` and reused across restarts. The index is incrementally updated when files change, which significantly speeds up startup for large projects.

## Vector Embeddings

When `search.embeddings: true` is set in `dna.yaml`, each indexed symbol also gets a vector embedding via `all-MiniLM-L6-v2` (384 dims, ONNX). Embedding text is built with **weighted concatenation**:

```
Name(3×) | Signature(2×) | Docstring(1×) | Body Keywords(0.5×)
```

Body keywords are unique identifiers extracted from function bodies (>3 chars, excluding language keywords and primitive types). For large functions (>20 lines), keywords are sampled from **three regions** (start, middle, end) to ensure representative coverage across the entire function body. This produces dense vectors that capture symbol semantics beyond just the name.

## Body Snippet Extraction

Indexed body snippets use a **sandwich strategy** for large functions:
- Functions ≤40 lines: captured in full
- Functions >40 lines: first 20 lines + `// ...` + last 20 lines

This ensures both the function signature/setup AND the closing definitions/return values are visible in the search index, improving recall for queries that match late-function content.

## MCP Integration

| Tool | Description |
| :--- | :--- |
| `search` | Search for code by concept. Supports fuzzy matching. |

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
    "name": "search",
    "arguments": {
      "query": "authentication login",
      "limit": 5
    }
  }
}
```
