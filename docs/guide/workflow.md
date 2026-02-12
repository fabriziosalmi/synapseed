# The Quantum Loop: High-Velocity Development with SYNAPSEED

> SYNAPSEED managing SYNAPSEED — the meta-workflow for iterative self-improvement.

This guide defines the operational playbook for using SYNAPSEED's MCP tools to
accelerate development cycles. Every prompt, every tool call, every gate is
designed to be copy-pasted into your AI coding session.

---

## Baseline (as of v2.0.2)

| Metric | Value |
|--------|-------|
| Architecture Score | **97/100 (Grade A)** |
| Modules | 131 |
| Edges | 44 |
| Violations | 1 (god\_object: `assets::graph` — 81 symbols) |
| Avg Instability | 0.16 |
| Max Coupling | 2 |
| Tests | 93 passing, 0 failing |
| MCP Surface | 19 tools, 8 resources, 6 prompts |

---

## The 19 MCP Tools — Organized by Role

### Tier 1: Orchestration (start here)

| Tool | When to Use | Latency |
|------|------------|---------|
| `ask_synapseed` | **Every session opener**. Natural-language triage: auto-routes to compiler, search, history, security, architecture. Returns enriched context + smart summary. | ~2s |

### Tier 2: Quality Gates (run at every cycle boundary)

| Tool | When to Use | Latency |
|------|------------|---------|
| `get_diagnostics` | Before every commit — zero-warning policy. Filter by file or severity. | <100ms |
| `architect_analyze` | After structural changes — block if new violations appear. Use `refresh: true` to bypass cache. | ~500ms |
| `scan_security` | Before any code leaves your machine — DLP check on config, env, credentials. | <50ms |
| `check_command` | Before running any shell command — Sentinel policy gate. | <10ms |

### Tier 3: Deep Analysis (use when investigating)

| Tool | When to Use | Latency |
|------|------------|---------|
| `analyze_history` | Before refactoring a file — shows churn score, co-change patterns, risk indicator. | ~200ms |
| `git_history` | Blame a specific line range — who changed what and why. | ~100ms |
| `git_intent_summary` | Understand the direction of recent work — groups commits by category. | ~150ms |
| `semantic_search` | Find symbols by concept (Tantivy keyword index). | ~50ms |
| `semantic_similarity` | Find code by meaning (vector embeddings, cosine similarity). Requires `search.embeddings: true`. | ~300ms |
| `lookup_symbol` | Exact symbol lookup — file path, line numbers, signature. | ~50ms |
| `get_code_skeleton` | AST skeleton of a directory — files, symbols, structure. | ~200ms |

### Tier 4: Automated Maintenance

| Tool | When to Use | Latency |
|------|------------|---------|
| `janitor_run_now` | Periodic debt scan — clippy warnings + unused deps. Runs async in background. | ~5s (bg) |
| `janitor_apply_fix` | Apply a Janitor proposal. **Always dry-run first** (`confirm: false`), then `confirm: true`. Auto-reverts on compile failure. | ~2s |
| `apply_quick_fix` | Apply rustc's `MachineApplicable` suggestions. Call `get_diagnostics` first to find error codes. | ~1s |

### Tier 5: Sandbox & Telemetry

| Tool | When to Use | Latency |
|------|------------|---------|
| `train_code` | Evaluate Rust code in isolated sandbox — compile, test, benchmark, fuzz. Use to compare variants. | 5-60s |
| `reset_telemetry` | Clear OTLP spans/metrics for a fresh observation window. | <10ms |
| `project_diagnose` | Full project diagnostic: state, build system, git, metrics, plugins. | ~200ms |
| `consult_architect` | Query the DNA policy — preferred libs, naming, workspace strategy. | <10ms |

---

## The Quantum Loop

A single iteration cycle, 30-45 minutes. Repeat until convergence.

```
    +---> [1. TRIAGE] -----> [2. GUARDRAIL] -----> [3. DEBT PASS]
    |                                                      |
    |       <---- [5. COMMIT] <---- [4. STABILIZE] <------+
    |                                       |
    +---------------------------------------+
```

### Step 1: Triage (2 min)

Open every session with a single `ask_synapseed` call:

```
ask_synapseed("top 3 risks in the current codebase with next best action for each")
```

This auto-invokes: compiler diagnostics, semantic search, history analysis,
security scan, and architecture check. The `smart_context` field gives you a
prioritized action list.

**Alternative triage prompts:**

```
ask_synapseed("what changed since last session and what broke")
ask_synapseed("next micro-refactor with maximum ROI and minimum risk")
ask_synapseed("is there any code that co-changes with router.rs that I should touch together")
```

### Step 2: Guardrail (1 min)

Run the structural gate. **Stop if new violations appear.**

```
architect_analyze(refresh: true)
```

Check:
- Score must not drop below current baseline (97)
- No new violations beyond the known ones
- `max_coupling` must stay <= 2

### Step 3: Debt Pass (5-10 min)

Run the Janitor for automated cleanup:

```
janitor_run_now()
```

Wait for background scan to complete, then review proposals:

```
janitor_apply_fix(proposal_id: "<id>", confirm: false)   // preview
janitor_apply_fix(proposal_id: "<id>", confirm: true)    // apply
```

**Rule:** Only apply low-risk fixes (single-file, no API changes).
Skip anything that touches public interfaces.

### Step 4: Stabilize (5 min)

Run the quality gates:

```
get_diagnostics(min_severity: "warning")    // must be CLEAN
analyze_history(file: "<hotspot file>")     // check risk indicator
scan_security("<any new config content>")   // DLP check
```

Files to always check before commit (current hotspots):
- `crates/whisper/src/router/mod.rs` — complexity 19, co-change hub
- `crates/mcp/src/tools/mod.rs` — 19-tool dispatch, high fan-out
- `crates/visualizer/assets/graph.js` — 81 symbols (god object remediation in progress)

### Step 5: Commit (2 min)

Small, focused commit. One objective per cycle.

Then loop back to Step 1.

---

## Ready-to-Use Prompt Pack

Copy-paste these into your AI coding session (Claude Code, Codex, etc.)
when SYNAPSEED MCP is connected.

### Session Openers

```
ask_synapseed("give me the current health snapshot: architecture score, open warnings, hotspot files, and suggested next action")
```

```
ask_synapseed("what are the top 3 files by churn in the last 10 commits, and which ones have co-change coupling")
```

### Pre-Refactor Investigation

```
analyze_history(file: "crates/whisper/src/router/mod.rs")
```

```
ask_synapseed("before modifying crates/mcp/src/tools/mod.rs, list all downstream dependents and minimum test set to run")
```

```
ask_synapseed("for the god object in graph.js, propose a migration order that maintains backwards compatibility at each step")
```

### During Implementation

```
get_diagnostics(min_severity: "warning")
```

```
semantic_search(query: "error handling pattern", limit: 5)
```

```
train_code(source: "<your code>", tests: "<your tests>", fuzz: true)
```

### Pre-Commit Checklist

```
get_diagnostics(min_severity: "info")
architect_analyze(refresh: true)
scan_security("<content of any new config file>")
```

### Periodic Maintenance (weekly)

```
janitor_run_now()
git_intent_summary(limit: 50)
ask_synapseed("run a full security audit across all source files")
```

---

## Anti-Patterns

| Anti-Pattern | Why It Hurts | Do This Instead |
|-------------|-------------|-----------------|
| Skipping `architect_analyze` after structural changes | Score drift goes unnoticed; violations compound silently | Gate every PR on score >= baseline |
| Using `janitor_apply_fix(confirm: true)` without preview | Applying blind fixes can break public API | Always dry-run first |
| Ignoring `analyze_history` risk indicator | Refactoring HIGH-risk files without context causes regressions | Check history before touching any file with > 5 commits |
| Running `ask_synapseed` with vague queries | Generic queries produce generic context — wasted tokens | Be specific: file paths, function names, intent |
| Never resetting telemetry | Heatmap accumulates stale data; hotspots become meaningless | `reset_telemetry` at the start of each focused session |

---

## Metrics to Track

After each Quantum Loop iteration, record:

1. **Architecture Score** — must be monotonically non-decreasing
2. **Warning Count** — target: 0 (currently 0 in Rust, pre-existing in husk/search)
3. **Test Count** — currently 93, should grow with each feature
4. **Violation Count** — currently 1, track toward 0
5. **Hotspot Risk** — monitor top 3 files by `analyze_history` risk indicator

---

## How SYNAPSEED Manages SYNAPSEED

This is the meta-loop. SYNAPSEED's own tools provide the telemetry and
guardrails needed to improve SYNAPSEED itself:

```
SYNAPSEED (the tool)
    |
    +--> architect_analyze    --> detects structural drift in SYNAPSEED's codebase
    +--> janitor_run_now      --> finds clippy/dep issues in SYNAPSEED's own code
    +--> get_diagnostics      --> shadow-compiles SYNAPSEED after every edit
    +--> analyze_history      --> identifies SYNAPSEED's own hotspot files
    +--> train_code           --> validates refactoring of SYNAPSEED's own modules
    +--> ask_synapseed        --> orchestrates all of the above in one call
    |
    +--> SYNAPSEED (improved)
```

This is not theoretical. Every refactoring session in this project uses this
exact loop. The document you're reading was generated from a real analysis
session where SYNAPSEED analyzed itself and produced actionable improvements.
