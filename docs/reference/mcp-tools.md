# MCP Tools

SYNAPSEED exposes 21 tools via the Model Context Protocol. Tools are callable actions that the LLM can invoke.

## `hoist`

Index a project directory and return its AST skeleton.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `path` | string | No | Directory to index (default: project root) |

**Returns:** JSON with `files_indexed`, `symbols_indexed`, `path`.

---

## `lookup`

Find a symbol by name across the entire project.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `name` | string | Yes | Symbol name to search for |

**Returns:** Array of matching symbols with file, line range, kind, and signature.

---

## `scan`

Scan text content for sensitive data (API keys, passwords, tokens, PII) and/or code vulnerability patterns (SQL injection, XSS, command injection, path traversal).

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `content` | string | Yes | Text to scan |
| `mode` | string | No | Scan mode: `all` (default — DLP + patterns), `dlp` (secrets only), `patterns` (code vulnerability patterns only) |

**Returns:** `CLEAN` or `ALERT` with findings and redacted output. When mode is `all`, combines DLP and code pattern results.

---

## `check`

Evaluate a shell command against the security policy.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `command` | string | Yes | Shell command to evaluate |

**Returns:** `ALLOWED` or `DENIED` with reason.

---

## `blame`

Get git blame/history for a file.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `file` | string | Yes | File path (relative to project root) |
| `start_line` | integer | No | Start line (default: 1) |
| `end_line` | integer | No | End line (default: 50) |

**Returns:** Array of blame entries with commit, author, timestamp, and message.

---

## `diagnose`

Run a full diagnostic on the project.

**Parameters:** None.

**Returns:** Project state, git status, recent commits, and metrics.

---

## `consult`

Consult the project's architecture policy.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `query` | string | Yes | Architecture question |

**Returns:** Architecture guidance from DNA configuration.

---

## `search`

Search for code by concept using Tantivy.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `query` | string | Yes | Semantic search query |
| `limit` | integer | No | Max results (default: 5) |

**Returns:** Ranked search results with file, symbol, score, and context.

---

## `diagnostics`

Get current compiler diagnostics from the background shadow compiler. Supports severity filtering.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `file` | string | No | Filter by file path |
| `min_severity` | string | No | Minimum severity: `info`, `warning` (default), `error` |

**Returns:** Errors and warnings with codes, messages, and suggestions. Returns `CLEAN` if no diagnostics match the filter.

---

## `analyze`

Analyze the full history of a file.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `file` | string | Yes | File path |
| `start_line` | integer | No | Scope to start line |
| `end_line` | integer | No | Scope to end line |

**Returns:** Churn score, co-change patterns, semantic tags, risk assessment.

---

## `quickfix`

Apply a compiler-suggested fix automatically.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `file` | string | Yes | File with the error |
| `error_code` | string | Yes | Error code to fix |

**Returns:** Success message or error. Only applies `MachineApplicable` suggestions.

---

## `ask`

The Intent Router. Ask a natural-language question and get an orchestrated response.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `query` | string | Yes | Natural-language question |
| `raw` | boolean | No | When true, inject the exact source code of discovered symbols into the prompt (Direct Symbol Injection). Default: false |

**Returns:** Enriched context with diagnostics, history, code context, security status, and SID (Semantic Information Density) metric. When `raw: true`, includes verbatim source code between `--- FILE: path (lines X-Y) ---` / `--- END ---` delimiters. Output format adapts to the detected model tier (Atomic/Molecular/Galactic).

---

## `reset-telemetry`

Clear all telemetry data from the OTLP receiver.

**Parameters:** None.

**Returns:** Confirmation with count of cleared spans and locations.

---

## `intent`

Summarize the intent and direction of recent commits semantically. Groups commits by category (fix, feature, refactor, security, etc.) and extracts scope hints from conventional commit messages.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `limit` | integer | No | Number of recent commits to analyze (default: 20) |

**Returns:** Natural-language summary with category breakdown and JSON detail.

---

## `train`

Evaluate Rust code in an isolated sandbox (The Gym). Compiles, tests, benchmarks, and optionally runs adversarial mutation testing, returning metrics and a composite score.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `source` | string | Yes | Rust source code to evaluate (injected as lib.rs) |
| `tests` | string | No | Optional test code (injected as tests/eval.rs). Use `use eval_project::*;` to import. |
| `timeout` | integer | No | Max evaluation time in seconds (default: 60) |
| `fuzz` | boolean | No | Enable proptest fuzzing: auto-generate property tests for public functions (default: false) |
| `adversarial` | boolean | No | Enable adversarial mutation testing: apply controlled mutations to measure test suite effectiveness (default: false) |

**Returns:** Compilation status, test results, mutation score (if adversarial), and detailed diagnostics with composite score.

---

## `janitor`

Run the Janitor: scan for clippy warnings and unused dependencies, generate validated fix proposals.

**Parameters:** None.

**Returns:** Findings and actionable proposals with UUIDs for `janitor-fix`.

---

## `janitor-fix`

Apply a specific Janitor fix proposal. **Dry-run by default** — shows a preview of what would change. Set `confirm: true` to actually apply.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `proposal_id` | string | Yes | UUID of the proposal (from `janitor`) |
| `confirm` | boolean | No | Set to `true` to apply. Default: `false` (preview only) |

**Returns:** Preview diff (dry-run) or success/error message (confirmed). Automatically reverts if compilation breaks.

---

## `architect`

Analyze project structural health: dependency graph, coupling metrics, cycle detection, god objects, layer violations.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `refresh` | boolean | No | Force fresh analysis (default: use cached report) |

**Returns:** Architecture score (A-F), violations, module metrics, and recommendations.

---

## `similar`

Find code similar to a natural-language query using vector embeddings (cosine similarity). Requires `search.embeddings: true` in DNA config.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `query` | string | Yes | Natural-language description of code to find |
| `top_k` | integer | No | Number of results (default: 5) |
| `min_similarity` | number | No | Minimum cosine similarity threshold (default: 0.3) |

**Returns:** Ranked results with file, symbol, similarity score, and code snippet.

---

## `oracle`

Auto-repair drifted documentation. Updates version numbers, crate counts, and MCP tool/resource counts in README.md to match the actual codebase.

**Parameters:** None.

**Returns:** List of changes made (or "no drift detected").

---

## `verify_path`

Verify whether a file path exists within the project. Prevents LLM hallucination of file names by providing ground truth.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `path` | string | Yes | File path to verify (relative to project root) |

**Returns:** `{ exists, size_bytes, language, is_file }`. Path traversal attempts are blocked.
