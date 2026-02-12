# The Quantum Loop: High-Velocity Development with SYNAPSEED

> SYNAPSEED managing SYNAPSEED — the meta-workflow for iterative self-improvement.

This guide defines the operational playbook for using SYNAPSEED's MCP tools to
accelerate development cycles. Every prompt, every tool call, every gate is
designed to be copy-pasted into your AI coding session.

---

## Baseline (as of v3.4.0)

| Metric | Value |
|--------|-------|
| Architecture Score | **100/100 (Grade A)** |
| Modules | 172 |
| Symbols | 1741 |
| Tests | 255 passing, 0 failing |
| MCP Surface | 20 tools, 9 resources, 6 prompts |

---

## The 20 MCP Tools — Organized by Role

### Tier 1: Orchestration (start here)

| Tool | When to Use | Latency |
|------|------------|---------|
| `ask` | **Every session opener**. Natural-language triage: auto-routes to compiler, search, history, security, architecture. Returns enriched context + smart summary. Use `--raw` for Direct Symbol Injection (source code in prompt). | ~2s |

### Tier 2: Quality Gates (run at every cycle boundary)

| Tool | When to Use | Latency |
|------|------------|---------|
| `diagnostics` | Before every commit — zero-warning policy. Filter by file or severity. | <100ms |
| `architect` | After structural changes — block if new violations appear. Use `refresh: true` to bypass cache. | ~500ms |
| `scan` | Before any code leaves your machine — DLP + code pattern check. Use `mode` param: `all`, `dlp`, or `patterns`. | <50ms |
| `check` | Before running any shell command — Sentinel policy gate. | <10ms |

### Tier 3: Deep Analysis (use when investigating)

| Tool | When to Use | Latency |
|------|------------|---------|
| `analyze` | Before refactoring a file — shows churn score, co-change patterns, risk indicator. | ~200ms |
| `blame` | Blame a specific line range — who changed what and why. | ~100ms |
| `intent` | Understand the direction of recent work — groups commits by category. | ~150ms |
| `search` | Find symbols by concept (Tantivy keyword index). | ~50ms |
| `similar` | Find code by meaning (vector embeddings, cosine similarity). Requires `search.embeddings: true`. | ~300ms |
| `lookup` | Exact symbol lookup — file path, line numbers, signature. | ~50ms |
| `hoist` | AST skeleton of a directory — files, symbols, structure. | ~200ms |

### Tier 4: Automated Maintenance

| Tool | When to Use | Latency |
|------|------------|---------|
| `janitor` | Periodic debt scan — clippy warnings + unused deps. Runs async in background. | ~5s (bg) |
| `janitor-fix` | Apply a Janitor proposal. **Always dry-run first** (`confirm: false`), then `confirm: true`. Auto-reverts on compile failure. | ~2s |
| `quickfix` | Apply rustc's `MachineApplicable` suggestions. Call `diagnostics` first to find error codes. | ~1s |

### Tier 5: Sandbox & Telemetry

| Tool | When to Use | Latency |
|------|------------|---------|
| `train` | Evaluate Rust code in isolated sandbox — compile, test, benchmark, fuzz, adversarial mutation testing. Use `adversarial: true` for mutation score. | 5-60s |
| `reset-telemetry` | Clear OTLP spans/metrics for a fresh observation window. | <10ms |
| `diagnose` | Full project diagnostic: state, build system, git, metrics, plugins. | ~200ms |
| `consult` | Query the DNA policy — preferred libs, naming, workspace strategy. | <10ms |

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

Open every session with a single `ask` call:

```
ask("top 3 risks in the current codebase with next best action for each")
```

This auto-invokes: compiler diagnostics, semantic search, history analysis,
security scan, and architecture check. The `smart_context` field gives you a
prioritized action list.

**Alternative triage prompts:**

```
ask("what changed since last session and what broke")
ask("next micro-refactor with maximum ROI and minimum risk")
ask("is there any code that co-changes with router.rs that I should touch together")
```

### Step 2: Guardrail (1 min)

Run the structural gate. **Stop if new violations appear.**

```
architect(refresh: true)
```

Check:
- Score must not drop below current baseline (100)
- No new violations
- `max_coupling` must stay <= 2

### Step 3: Debt Pass (5-10 min)

Run the Janitor for automated cleanup:

```
janitor()
```

Wait for background scan to complete, then review proposals:

```
janitor-fix(proposal_id: "<id>", confirm: false)   // preview
janitor-fix(proposal_id: "<id>", confirm: true)    // apply
```

**Rule:** Only apply low-risk fixes (single-file, no API changes).
Skip anything that touches public interfaces.

### Step 4: Stabilize (5 min)

Run the quality gates:

```
diagnostics(min_severity: "warning")    // must be CLEAN
analyze(file: "<hotspot file>")     // check risk indicator
scan("<any new config content>")   // DLP check
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
ask("give me the current health snapshot: architecture score, open warnings, hotspot files, and suggested next action")
```

```
ask("what are the top 3 files by churn in the last 10 commits, and which ones have co-change coupling")
```

### Pre-Refactor Investigation

```
analyze(file: "crates/whisper/src/router/mod.rs")
```

```
ask("before modifying crates/mcp/src/tools/mod.rs, list all downstream dependents and minimum test set to run")
```

```
ask("for the god object in graph.js, propose a migration order that maintains backwards compatibility at each step")
```

### During Implementation

```
diagnostics(min_severity: "warning")
```

```
search(query: "error handling pattern", limit: 5)
```

```
train(source: "<your code>", tests: "<your tests>", fuzz: true, adversarial: true)
```

### Pre-Commit Checklist

```
diagnostics(min_severity: "info")
architect(refresh: true)
scan("<content of any new config file>")
```

### Periodic Maintenance (weekly)

```
janitor()
intent(limit: 50)
ask("run a full security audit across all source files")
```

---

## Anti-Patterns

| Anti-Pattern | Why It Hurts | Do This Instead |
|-------------|-------------|-----------------|
| Skipping `architect` after structural changes | Score drift goes unnoticed; violations compound silently | Gate every PR on score >= baseline |
| Using `janitor-fix(confirm: true)` without preview | Applying blind fixes can break public API | Always dry-run first |
| Ignoring `analyze` risk indicator | Refactoring HIGH-risk files without context causes regressions | Check history before touching any file with > 5 commits |
| Running `ask` with vague queries | Generic queries produce generic context — wasted tokens | Be specific: file paths, function names, intent |
| Never resetting telemetry | Heatmap accumulates stale data; hotspots become meaningless | `reset-telemetry` at the start of each focused session |

---

## Metrics to Track

After each Quantum Loop iteration, record:

1. **Architecture Score** — must be monotonically non-decreasing
2. **Warning Count** — target: 0 (currently 0 in Rust, pre-existing in husk/search)
3. **Test Count** — currently 93, should grow with each feature
4. **Violation Count** — currently 1, track toward 0
5. **Hotspot Risk** — monitor top 3 files by `analyze` risk indicator

---

## How SYNAPSEED Manages SYNAPSEED

This is the meta-loop. SYNAPSEED's own tools provide the telemetry and
guardrails needed to improve SYNAPSEED itself:

```
SYNAPSEED (the tool)
    |
    +--> architect    --> detects structural drift in SYNAPSEED's codebase
    +--> janitor      --> finds clippy/dep issues in SYNAPSEED's own code
    +--> diagnostics      --> shadow-compiles SYNAPSEED after every edit
    +--> analyze      --> identifies SYNAPSEED's own hotspot files
    +--> train           --> validates refactoring of SYNAPSEED's own modules
    +--> ask        --> orchestrates all of the above in one call
    |
    +--> SYNAPSEED (improved)
```

This is not theoretical. Every refactoring session in this project uses this
exact loop. The document you're reading was generated from a real analysis
session where SYNAPSEED analyzed itself and produced actionable improvements.
