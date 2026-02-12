# MCP Tools

SYNAPSEED exposes 14 tools via the Model Context Protocol. Tools are callable actions that the LLM can invoke.

## `get_code_skeleton`

Index a project directory and return its AST skeleton.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `path` | string | No | Directory to index (default: project root) |

**Returns:** JSON with `files_indexed`, `symbols_indexed`, `path`.

---

## `lookup_symbol`

Find a symbol by name across the entire project.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `name` | string | Yes | Symbol name to search for |

**Returns:** Array of matching symbols with file, line range, kind, and signature.

---

## `scan_security`

Scan text content for sensitive data (API keys, passwords, tokens, PII).

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `content` | string | Yes | Text to scan |

**Returns:** `CLEAN` or `ALERT` with findings and redacted output.

---

## `check_command`

Evaluate a shell command against the security policy.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `command` | string | Yes | Shell command to evaluate |

**Returns:** `ALLOWED` or `DENIED` with reason.

---

## `git_history`

Get git blame/history for a file.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `file` | string | Yes | File path (relative to project root) |
| `start_line` | integer | No | Start line (default: 1) |
| `end_line` | integer | No | End line (default: 50) |

**Returns:** Array of blame entries with commit, author, timestamp, and message.

---

## `project_diagnose`

Run a full diagnostic on the project.

**Parameters:** None.

**Returns:** Project state, git status, recent commits, and metrics.

---

## `consult_architect`

Consult the project's architecture policy.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `query` | string | Yes | Architecture question |

**Returns:** Architecture guidance from DNA configuration.

---

## `semantic_search`

Search for code by concept using Tantivy.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `query` | string | Yes | Semantic search query |
| `limit` | integer | No | Max results (default: 5) |

**Returns:** Ranked search results with file, symbol, score, and context.

---

## `get_diagnostics`

Get current compiler diagnostics from the shadow compiler.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `file` | string | No | Filter by file path |

**Returns:** Errors and warnings with codes, messages, and suggestions.

---

## `analyze_history`

Analyze the full history of a file.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `file` | string | Yes | File path |
| `start_line` | integer | No | Scope to start line |
| `end_line` | integer | No | Scope to end line |

**Returns:** Churn score, co-change patterns, semantic tags, risk assessment.

---

## `apply_quick_fix`

Apply a compiler-suggested fix automatically.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `file` | string | Yes | File with the error |
| `error_code` | string | Yes | Error code to fix |

**Returns:** Success message or error. Only applies `MachineApplicable` suggestions.

---

## `ask_whisperer`

The Intent Router. Ask a natural-language question and get an orchestrated response.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `query` | string | Yes | Natural-language question |

**Returns:** Enriched context with diagnostics, history, code context, and security status.

---

## `reset_telemetry`

Clear all telemetry data from the OTLP receiver.

**Parameters:** None.

**Returns:** Confirmation with count of cleared spans and locations.

---

## `git_intent_summary`

Summarize the intent and direction of recent commits semantically. Groups commits by category (fix, feature, refactor, security, etc.) and extracts scope hints from conventional commit messages.

**Parameters:**
| Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `limit` | integer | No | Number of recent commits to analyze (default: 20) |

**Returns:** Natural-language summary with category breakdown and JSON detail.
