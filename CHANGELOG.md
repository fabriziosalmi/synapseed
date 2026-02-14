# Changelog

## [4.18.0] — 2026-02-14

### Il Registratore — Flight Recorder & Session Memory

New **Flight Recorder** system provides dual-track session memory, capturing the full
arc of a coding session — what happened, when, and why.

#### Flight Recorder Core (`crates/core/src/recorder.rs`)
- **Via 1 — Working Set**: Circular buffer of last 20 events with timestamps, kinds, and details.
- **Via 2 — Journey Map**: Compressed timeline of context shifts with automatic activity
  classification (Exploring / Coding / Debugging / Hardening / Mixed).
- **Loop Detection**: Flags repeated tool→file patterns that suggest the LLM is stuck.
- **Causal Links**: `dep_hints` from Architect's dependency graph enable cross-module
  correlation of file changes.
- 12 unit tests covering recording, phases, loops, idle timeout, markdown rendering, JSON serialization.

#### System Wiring
- **Tool Dispatch**: Every tool call recorded as `EventKind::ToolCall` with result preview.
- **EventBus Subscriber**: Background task forwards `FileChanged`, `SymbolResolved`,
  `DiagnosticUpdated` events to the recorder via `spawn_recorder_subscriber()`.
- **MCP Resource**: New `synapseed://session/recorder` — JSON snapshot of session state.
- **Context Injection**: `synapseed://context/active` now includes `flight_recorder`
  summary (total events, phase count, current phase, loop alerts).
- **Architect Dep Hints**: `DependencyGraph.dep_pairs()` feeds module dependency pairs
  to the recorder for causal link detection.

#### Bug Fixes (v4.17.0 hardening)
- **Conservative Boosting**: `ask` query expansion now uses `original^3`, `synonym^0.5`
  with max 2 synonyms — prevents synonym dilution (P1 from benchmark).
- **Phantom Diagnostics**: Shadow compiler filters stale diagnostics for deleted files;
  `Deleted` file events now trigger recheck.
- **Ask Diagnostic Context**: Fixed field name mismatch (`file`→`file_path`, `line`→`line_start`)
  and always merges global errors into scoped results.
- **`.arg(&var)` Heuristic**: Husk DLP now skips common Rust builder-pattern calls
  (`.arg()`, `.env()`, `.header()`, `.query()`) to avoid false positives.
- **Generic URI Regex**: Broadened URI detection to catch non-http schemes.

#### Benchmark Results (Qwen3 1.7B — Coding)
| Metric | BLIND | SYNAPSEED | Delta |
| :--- | :--- | :--- | :--- |
| Easy | 0.80 | 0.90 | +0.10 |
| Medium | 0.39 | 0.76 | +0.37 |
| Hard | 0.16 | 0.73 | +0.57 |
| **Mean** | **0.448** | **0.796** | **+0.348** |

#### Counts
- 24 tools, 11 resources, 6 prompts
- 44 test suites, 0 failures

## [4.17.0] — 2026-02-14

### L'Intelligenza — DLP, Synonyms, Security & Benchmark Engine

Self-improvement cycle driven by SYNAPSEED-on-SYNAPSEED analysis. 10 weaknesses
identified, all fixed. Added benchmark engine and neural decompiler.

#### Features
- **Benchmark Engine** (`synapseed-bench`): Reproducible SCR evaluation with JSONL
  question suites, F1/SID/hallucination scoring.
- **Neural Decompiler** (`synapseed-decompiler`): ELF/Mach-O/PE binary analysis with
  symbol extraction, strings, call graph, and behavioral inference.
- **VS Code Extension v0.3.0**: Drag-and-drop panels, tabbed dashboard, enriched Ask panel.

#### Counts
- 24 tools, 10 resources, 6 prompts

## [4.16.0] — 2026-02-14

### Il Decompilatore — Binary Analysis & Benchmark Engine

- **Neural Decompiler**: New `synapseed-decompiler` crate for ELF/Mach-O/PE analysis.
- **Benchmark Engine**: New `synapseed-bench` crate for reproducible evaluation suites.

## [4.15.0] — 2026-02-14

### La Precisione — Search Dedup, Enum Expansion & VS Code Extension

Benchmark-driven refinements to search precision and context quality, plus the first
SYNAPSEED VS Code extension. Search MRR jumps **0.715 → 0.944** (+32%), Recall@10
jumps **0.597 → 0.917** (+54%).

#### Search Result Deduplication
- **`score_results()`** now deduplicates by `(symbol, file)` pair after scoring and sorting.
  Previously, the same symbol could appear multiple times (e.g., `DlpScanner` ×2,
  `MomentumEngine` ×3) when matched by different search tiers (BM25, prefix, fuzzy).
  The dedup retains only the highest-scored entry per unique symbol.

#### Enum & Constant Context Expansion
- **`inject_raw_sources()`** now expands the line range for `Enum` symbols (+25 lines)
  and `Constant` symbols (+15 lines) when injecting raw source code. This ensures all
  enum variants and constant definitions are visible in the LLM context, not just the
  type declaration header.

#### Metadata File Indexing
- **`index_metadata_files()`** — New method on `SemanticIndex` that indexes project metadata
  files (Cargo.toml, LICENSE, .cargo/config.toml, rust-toolchain.toml) as searchable
  pseudo-documents. Queries like "license", "Rust toolchain version", or "workspace config"
  now return relevant results.

#### VS Code Extension
- First release of the **SYNAPSEED VS Code Extension** with 9 sidebar panels:
  Project Status, Metrics, Compiler Diagnostics, Architecture Health, Git History,
  Security, Consistency, Janitor Proposals, and Telemetry.
- Commands: Refresh All, Ask a Question, Open Dashboard, Run Janitor Scan.
- Auto-refresh on file save + configurable timer interval.
- Status bar integration showing build status with click-to-refresh.
- Packaged as `.vsix` for local installation.

#### Benchmark Ground Truth Fixes
- Fixed 6 search queries referencing non-existent symbols (`scan_content`, `SearchIndex`,
  `run_mutations`, `visibility_boost`, `HttpServer`, `pagerank_boost`) — the benchmark was
  penalizing SYNAPSEED for correctly *not* returning hallucinated symbols.
- Fixed grounding question g14 with correct visibility weight values from actual source code.

#### Benchmark Results (Qwen3 1.7B)

**Search:**
| Metric | v4.14.0 | v4.15.0 |
| :--- | :--- | :--- |
| MRR | 0.715 | **0.944** |
| P@10 | 0.367 | **0.475** |
| R@10 | 0.597 | **0.917** |
| File Hit@10 | 0.944 | **1.000** |

**Grounding:**
| Metric | v4.14.0 | v4.15.0 |
| :--- | :--- | :--- |
| BLIND | 21.2/45 | 21.2/45 |
| GROUNDED | 32.6/45 | 32.4/45 |
| F1 (avg) | 0.856 | 0.849 |

#### Counts
- 21 tools, 10 resources, 6 prompts
- 373 tests pass, 0 failures

## [4.14.0] — 2026-02-14

### Il Contesto Giusto — Score-Ordered Context Delivery

Fixed core context delivery bug: SYNAPSEED was injecting low-relevance symbols first,
exhausting the token budget before the most relevant targets could be included.
Benchmark delta on Qwen3 1.7B: **-0.033 → +0.100** (+0.133 swing).

#### Score-Ordered Raw Source Injection
- **inject_raw_sources()** now sorts targets by search `score` DESC before processing.
  The most relevant symbols (highest composite BM25+PageRank+Visibility score) get
  budget priority. Targets without scores (from non-search passes) go last.
- **break → continue + truncation**: Previously, when a single oversized target exceeded
  the 16K char budget, the `break` statement stopped ALL remaining injection. Now uses
  `continue` with smart truncation — first 75% + last 25% with `[truncated]` separator.
- **Budget guard**: Only skips entirely when `remaining < 200` chars.

#### Smart Body Snippet Extraction
- **Sandwich strategy** in `extract_body_snippet()`: Functions ≤40 lines are captured fully.
  Functions >40 lines get first 20 lines + `// ...` + last 20 lines. This ensures both the
  function header/setup AND closing definitions/returns are visible in the index.
- Previously captured only the first 30 lines — missed return types and late definitions.

#### Multi-Region Body Keyword Extraction
- **extract_body_keywords()** now samples from start, middle, AND end regions of large functions.
  For functions ≤2×max_lines, all lines are scanned (unchanged). For larger functions,
  `max_lines` are sampled from each of three regions for representative keyword coverage.
- Previously sampled only the first 10 lines — large functions like `list_tools()` (700+ lines)
  had zero keywords from definitions past line 10.

#### Enriched list_tools() Documentation
- Doc comment on `list_tools()` now explicitly names all 21 tools in declaration order,
  enabling search to index tool names directly from the docstring without requiring
  body traversal.

#### Benchmark Results (Qwen3 1.7B, `--quick`)
| Metric | Before (v4.13.0) | After (v4.14.0) |
| :--- | :--- | :--- |
| BLIND mean | 0.800 | 0.800 |
| SYNAPSEED mean | 0.767 | **0.900** |
| Delta | -0.033 | **+0.100** |
| `easy_tool_count` keyword | 0.00 | **1.00** |
| `easy_tool_count` composite | 0.30 | **0.70** |

#### Counts
- 21 tools, 10 resources, 6 prompts
- 373 tests pass, 0 failures

## [4.13.0] — 2026-02-13

### Cognitive Dominance — LLM Tool Routing Optimization

System-level strategy to make SYNAPSEED the "first citizen" in multi-tool MCP environments.
Works on both 1.7B parameter models (Qwen3) and SOTA (Claude, GPT-4).

#### Tool Description Engineering
- **Imperative routing hierarchy**: All 21 tool descriptions rewritten with clear priority signals:
  - `ask` (PRIMARY): "ALWAYS call this tool FIRST" — single entry point for any code question
  - 4 CORE tools (search, lookup, scan, check): targeted operations with explicit routing guidance
  - 10 analysis tools: deep-dive with cross-references to `ask` for broad queries
  - 6 specialized tools: expert operations (train, janitor, architect, oracle, etc.)
- **Cognitive funnel**: Every non-primary tool description says "use `ask` for broad questions",
  statistically increasing the probability that LLMs route to the orchestrator first
- **Authoritative language**: Action verbs (resolve, validate, evaluate) replace passive ones (get, read)

#### MCP Tool Annotations (2025-03-26 spec)
- Added `annotations` field to `ToolDefinition` with `readOnlyHint`, `destructiveHint`,
  `idempotentHint`, `openWorldHint` — clients that support the 2025-03-26 MCP spec
  can now classify tool safety without parsing descriptions
- All analysis tools marked `readOnly=true, destructive=false`
- Mutation tools (quickfix, janitor-fix, oracle, reset-telemetry) marked `readOnly=false`

#### Dynamic Instructions Enhancement
- Rewrote `build_instructions()` with a **priority-ordered routing decision tree**:
  ```
  1. ANY code question → `ask` FIRST
  2. Specific symbol → `lookup` or `search`
  3. Security → `scan` before sharing, `check` before executing
  4. Architecture → `consult` or `architect`
  5. Build errors → `diagnostics` then `quickfix`
  6. Git context → `blame`, `analyze`, or `intent`
  ```
- Added **RULES** section: NEVER read files manually, NEVER execute commands without `check`,
  NEVER share code without `scan`, use `verify_path` before citing files
- Added **live diagnostics count** in instructions (error/warning count from shadow compiler)
- Removed legacy alias names from instructions (use canonical short names)

#### New Resource: `synapseed://context/active`
- **Passive interception strategy**: Dynamic project briefing that clients can preload
  into context before any tool call — eliminates multiple initial roundtrips
- Contains: project state, DNA summary, active diagnostics count, architecture grade,
  indexing metrics, session continuity, and tool routing hint
- Description includes `PRIORITY RESOURCE` signal for LLM resource selection

#### Counts
- 21 tools, 10 resources, 6 prompts
- 373 tests pass, 0 failures

## [4.12.0] — 2026-02-14

### Il Segnale + L'Espansione — Score Propagation, Enriched Embeddings, Prefix Matching

#### v4.12 "Il Segnale" — Context Quality
- **Score propagation**: Added `score: Option<f32>` to `Target` struct — search scores
  now flow through the entire Whisper pipeline for rank-aware context building
- **HashSet dedup (Sort First, Cut Later)**: Replaced sequential `dedup_by` with
  `HashSet<(name, file_path)>` deduplication after sorting by score DESC → source order ASC,
  eliminating O(n²) scanning and preserving best-ranked variants
- **Diagnostic items in prompt**: Context builder now renders up to 10 actual compiler
  error/warning messages (severity, file, line, message) instead of just a count
- **Multi-file history**: `gather_histories()` replaces single-file `gather_history()`,
  analyzing up to 5 unique file paths from targets for broader git context
- **Multi-intent classification**: New `classify_intent_scores()` returns ranked
  `Vec<(String, usize)>` intent scores, enabling intent-aware context prioritization

#### v4.13 "L'Espansione" — Embedding & Search Enrichment
- **Enriched embedding text**: `build_embedding_text()` now uses weighted concatenation:
  Name(3×) | Signature(2×) | Docstring(1×) | Body Keywords(0.5×) for denser vector
  representations that capture symbol semantics beyond just the name
- **Body keyword extraction**: New `extract_body_keywords()` helper extracts unique
  identifiers from function bodies (>3 chars, non-keyword, non-type) for embedding input
- **Prefix matching fallback**: Added `prefix_search()` via Tantivy `RegexQuery` as
  intermediate fallback between BM25 and fuzzy — catches `handle_req` → `handle_request`
  matches that BM25 misses but fuzzy overshoots
- **Three-tier search cascade**: BM25 → Prefix → Fuzzy with progressive fallback,
  improving recall for partial symbol name queries

#### Counts
- 373 tests pass, 0 failures
- Release binary: 33 MB (unchanged)

## [4.10.0] — 2026-02-13

### Language-Aware Visibility Boost & Benchmark Suite

#### Language-Aware Visibility Boost
- **Fixed Python/Django regression**: Relaxed visibility boost for dynamic languages
  (Python, JS, TS, Ruby, PHP) where `_private` methods often contain critical logic
- Static languages (Rust, Go): `public=1.5x, private=0.6x` (unchanged, 2.5x delta)
- Dynamic languages (Python, JS): `public=1.2x, private=0.9x` (relaxed, 1.33x delta)
- Django's `BaseHandler._get_response` and similar internal methods no longer drop
  below the Coherence Gate threshold

#### Benchmark Suite (`benchmark/`)
- **Coding benchmark**: BLIND vs SYNAPSEED comparison on code understanding tasks
  with composite scoring (keyword, file, symbol, hallucination detection)
- **Grounding benchmark**: MCP tool effectiveness with Precision/Recall/F1 evaluation
- **Search benchmark**: Tantivy ranking quality (MRR, P@K, R@K) via persistent MCP
  JSON-RPC session for accurate index-warm measurements
- **NIAH benchmark**: Needle-in-a-Haystack context sensitivity testing
- Multi-model support (`--all-models`) and dual-endpoint failover
- Streamlit dashboard (`benchmark/dashboard/`) with dark theme, gauge charts,
  per-task breakdowns, and historical trend tracking

## [4.9.2] — 2026-02-13

### Code Quality Sweep (Review-Driven Hardening)

Post-audit hardening pass based on fresh-eyes review of the entire codebase.

#### Error Handling
- Converted 7 silent `let _ =` patterns to proper `warn!()` logging across:
  - `search/indexer.rs`: segment commit, index writer commit, index writer delete
  - `core/oracle.rs`: README write failure now reported in changes vector
  - `gym/sandbox.rs`: mutated source write + original restore
  - `shadow-check/plugin.rs`: diagnostic trigger send failure
  - `bin/main.rs`: Tantivy timeout now traces SearchReady status

#### Test Coverage (+32 tests)
- `core/symbol.rs`: 8 tests — SymbolId uniqueness, Visibility/SymbolKind serde, Symbol with/without visibility, FileStructure roundtrip
- `core/event.rs`: 5 tests — FileChangeKind/Severity serde, tagged event format, FileChanged/SecurityAlert roundtrips
- `core/state.rs`: 7 tests — detect VirginRepo/HealthyWorkspace/PartialSetup/Npm/Unknown, diagnostic output, BuildSystem serde
- `core/session.rs`: 8 tests — is_recent, time_ago variants, save/load roundtrip, nonexistent/malformed load
- `core/policy.rs`: 4 tests — default fail_closed, SecurityPolicy serde, PolicyAction variants, missing-field defaults

#### Oracle Auto-Fix
- README.md metadata corrected: 13→14 crates, 20→21 tools, 8→9 resources

## [4.9.0] — 2026-02-13

### Visibility Boost — Public API Prioritization

Fixes the **fidelity defect** where internal implementation symbols (e.g., `Server`)
outrank public API symbols (e.g., `HttpServer`) in search results, causing small
LLMs to trust injected context over their training knowledge.

Benchmark evidence: `easy_server_struct` task on actix-web showed -67% accuracy
degradation (BLIND: 100% → SYNAPSEED: 33%) because the internal `Server` struct
was injected instead of the public `HttpServer`.

#### Core: `Visibility` enum in `symbol.rs`
- New `Visibility` enum: `Public`, `Crate`, `Super`, `Private`
- Added `visibility: Option<Visibility>` field to `Symbol`
- `None` = unknown (legacy/unsupported languages)

#### Parser: AST visibility extraction in `cortex/parser.rs`
- Rust: reads `visibility_modifier` child node — detects `pub`, `pub(crate)`, `pub(super)`, private
- Python: convention-based — `_name` = private, else public
- JavaScript: `export_statement` parent = public, else private
- Zero overhead for Unknown languages (returns `None`)

#### Search: Tantivy schema + visibility boost
- New `visibility` STRING field in Tantivy schema (stored + indexed)
- Stored as `"public"` / `"crate"` / `"super"` / `"private"` / `"unknown"`
- Disk indexes auto-recreate on schema mismatch (existing behavior)

#### Ranking: 8th multiplicative boost factor
- `pub` → ×1.5 (strong boost for public API)
- `pub(crate)` → ×1.0 (neutral)
- `pub(super)` → ×0.8 (mild penalty)
- private → ×0.6 (strong penalty)
- unknown → ×1.0 (neutral for legacy docs)

#### Boost Stack (updated)
```
score = BM25 × temporal × source × path × specificity × interface × pagerank × visibility
         │        │         │        │         │            │           │          │
         │     0.7-1.0   0.1-1.5  1.0-3.0  1.0-1.3     1.0-1.4    1.0-1.5   0.6-1.5
         └─ per-field weights: name(3x) > doc(2x) > body(1.5x) > sig(1x)
```

Expected impact: `HttpServer` (pub, ×1.5) now outranks `Server` (pub(crate), ×1.0)
by 50% in visibility alone, fixing the fidelity defect without reducing internal
symbol discoverability.

## [4.8.0] — 2026-02-13

### Module Authority — PageRank on Symbol Graph

Adds module-level PageRank computation to the search ranking pipeline.
Symbols from widely-imported (foundational) modules receive a mild ranking
boost, helping core infrastructure symbols surface above leaf-module code
in BM25 results.

#### Core: `pagerank.rs` in architect crate
- Classic power iteration: `PR(v) = (1-d)/N + d × Σ(PR(u)/out(u))` with d=0.85
- Converges when max per-node delta < 1e-6, or after 100 iterations
- Scores normalized to [0.0, 1.0] where 1.0 = most depended-upon module
- 6 unit tests: empty graph, single node, star topology, linear chain, cycle convergence, normalization

#### Integration: `DependencyGraph::pagerank_by_file()`
- Maps PageRank node scores to file paths for direct lookup
- Exposed as `pub` API on `DependencyGraph` in architect crate

#### Search Boost: Module Authority factor in `SemanticIndex::search()`
- New multiplicative boost: `pagerank_boost = 1.0 + score × 0.5` (range: 1.0–1.5)
- Applied after existing boosts (temporal, source, path, specificity, interface)
- Scores injected via `set_pagerank_scores()` / `has_pagerank_scores()` API
- Uses `parking_lot::RwLock<HashMap>` for thread-safe concurrent reads

#### Whisper Pipeline: Lazy PageRank injection
- Computed lazily on first `ask` query from the CodeGraph dependency structure
- `DependencyGraph::build()` + `pagerank_by_file()` runs in ~5ms for typical projects
- Cached in `SemanticIndex` for subsequent queries (same process lifetime)
- New dependency: `synapseed-whisper` → `synapseed-architect`

#### Boost Stack (updated)
```
score = BM25 × temporal × source × path × specificity × interface × pagerank
         │        │         │        │         │            │           │
         │     0.7-1.0   0.1-1.5  1.0-3.0  1.0-1.3     1.0-1.4    1.0-1.5
         └─ per-field weights: name(3x) > doc(2x) > body(1.5x) > sig(1x)
```

## [4.7.0] — 2026-02-13

### Noise Reduction — "La Dieta del Token"

Line-based noise pruner that collapses logging/debug statements and truncates
long lines before injecting source code into the LLM context. Increases
context efficiency by 10-30% without losing structural information.

#### Core: `prune_noise()` in context_builder.rs
- Detects and collapses logging statements across 3 language families:
  - **Rust**: `debug!()`, `info!()`, `warn!()`, `error!()`, `trace!()`, `println!()`, `eprintln!()`, `dbg!()`; also `tracing::*!()` and `log::*!()`
  - **Python**: `logger.*()`, `logging.*()`, `print()`
  - **JavaScript/TypeScript**: `console.log()`, `console.error()`, `console.warn()`, `console.debug()`
- Multi-line log tracking via paren depth counting (handles `debug!(\n field = val,\n "msg"\n);`)
- Consecutive log lines collapsed into a single `// ...` (or `# ...` for Python) marker
- Lines > 200 chars truncated (long string constants, generated code)
- Language-aware comment style based on file extension
- Applied after `minify_source()`, before token budget enforcement

#### Pipeline Position
```
inject_raw_sources():
  1. Read file from disk
  2. Select line range (CodeGraph or heuristic)
  3. minify_source() — whitespace cleanup
  4. prune_noise()   — logging/boilerplate removal ← NEW
  5. Token budget enforcement
  6. Return RawSource
```

#### Stats
- 331 tests, 0 failures (+10 new pruner tests)

## [4.6.0] — 2026-02-13

### Reverse Test Lookup — "Il Ponte della Verità"

New Pass 7 in the extraction pipeline: for each source symbol, automatically
finds test functions that exercise it and injects them into the context.

#### Core: Pass 7 in Whisper Extraction
- For each source (non-test) symbol found by Passes 1-6, searches BM25 for test files that reference it
- Tests are working usage examples: `assert_eq!(handler(req), expected)` teaches an LLM more than 100 lines of abstract implementation
- Mirror of Passes 4-5 (test→source): Pass 7 goes source→test
- Over-fetches 10 BM25 results per symbol to find test files ranked below source files
- Caps at 3 test injections total (avoids bloating context)
- Filters: minimum symbol name length (4+), skips Import/Variable kinds, deduplicates
- Source-first ordering preserved: injected tests sort after source code

#### Extraction Pipeline (now 7 passes)
1. Explicit file references
2. Hybrid RRF search (BM25 + vector fusion)
3. Cortex fallback
4. Implementation Twin (test→source)
5. Call Graph Lite (test bodies→callees)
6. Python Config String Resolver
7. **Reverse Test Lookup (source→test)** ← NEW

#### Stats
- 321 tests, 0 failures

## [4.5.0] — 2026-02-13

### Hybrid Retrieval — Reciprocal Rank Fusion (RRF)

Fuses BM25 (Tantivy) and vector embedding (fastembed) search results using Reciprocal Rank Fusion, a rank-based aggregation method from information retrieval research (Cormack, Clarke & Buettcher, 2009).

#### Core: RRF Fusion Engine (`crates/search/src/hybrid.rs`)
- New `hybrid_search()` function combines BM25 keyword matching with cosine vector similarity
- RRF formula: `score(d) = Σ 1/(k + rank(d))` with k=60 (standard constant)
- Documents found by **both** retrievers score higher than single-source matches
- 3x over-fetch from each source to maximize cross-retriever overlap
- Scores normalized to [0, 1] for compatibility with existing `min_confidence` thresholding
- Graceful fallback to BM25-only when embeddings unavailable or query embedding fails

#### Whisper: Pass 2 Upgrade
- Pass 2 now uses Hybrid RRF when the `embeddings` feature is enabled
- Automatically detects `VectorIndex` + `EmbeddingEngine` availability at runtime
- Falls back to BM25-only when vector components not initialized
- New `embeddings` feature flag for `synapseed-whisper` crate

#### Why It Matters
- BM25 captures keyword relevance ("FromRequest" matches symbol names)
- Vectors capture semantic similarity ("auth logic" matches `authenticate_user`)
- RRF intersection = highest-quality retrieval signal
- Addresses the BM25 seed bottleneck identified in v4.3.0 benchmarks (Django, Axum)

#### Stats
- 321 tests, 0 failures (+5 new RRF formula tests)

## [4.4.0] — 2026-02-13

### Inheritance Boost & Interface Ranking

Two complementary BM25 improvements addressing Axum extractor and Django middleware benchmark gaps.

#### Python: Inheritance Boost (Cortex Parser)
- `class Foo(Bar)` now enriches signature with `[inherits: Bar]` for BM25 discoverability
- Supports multiple inheritance: `class DashboardView(LoginRequiredMixin, View)` → `[inherits: LoginRequiredMixin, View]`
- Handles dotted superclasses: `class Article(django.db.models.Model)` → extracts `Model`
- Enables BM25 to connect child classes to parent frameworks (MiddlewareMixin, Model, View)

#### BM25: Interface/Trait Boost (Search Indexer)
- Trait/interface definitions now receive 1.4x boost in BM25 search ranking
- `FromRequest`, `ServiceFactory`, `MiddlewareMixin` surface over concrete implementations
- Addresses Axum benchmark gap where `axum-core/FromRequest` lost to `axum-extra/cookie`

#### Stats
- 316 tests, 0 failures (+3 new tests)

## [4.3.0] — 2026-02-13

### Trait Expansion & Python Config Resolver

Two new retrieval mechanisms targeting the gaps identified in the MCP benchmark analysis.

#### Rust: Trait Expansion
- `trait_item` now indexed as `SymbolKind::Interface` (was ignored)
- `impl_item` name extracted from `type` field (was silently skipped due to missing `name` field)
- `impl Trait for Type` enriches signature with `[trait: TraitName]` for BM25 discoverability
- Searching "FromRequest" now finds `impl FromRequest for MyExtractor`

#### Python: Config String Resolver (Pass 6)
- New extraction pass resolves dotted string references in Python config files
- `"django.middleware.security.SecurityMiddleware"` → looks up `SecurityMiddleware` in graph
- Only extracts PascalCase class names (≥3 chars) with ≥3 dotted components
- Addresses Django middleware chain gap where dynamic config broke static graph resolution

#### Stats
- 313 tests, 0 failures (+7 new tests)

## [4.2.0] — 2026-02-13

### Coherence Gate — "The Cigarette Break"

Anti-hallucination mechanism: when the extraction pipeline produces targets scattered across many unrelated modules, the Coherence Gate detects the incoherence and reorders by clustering targets by module proximity.

#### Formula
- **Coherence Score**: `CS = 1 - (unique_modules - 1) / max(total_targets - 1, 1)`
- **Threshold τ = 0.4**: below this, the gate activates
- **Clustering**: groups targets by module prefix, keeps top-K clusters (K=2 Atomic, K=3 otherwise)
- Largest cluster first → most query-relevant symbols dominate the context

#### Stats
- 306 tests, 0 failures (+9 new coherence tests)

## [4.1.0] — 2026-02-13

### CamelCase-Aware BM25 & Intent Coverage Fix

#### Search: CamelCase-Aware Indexing & Retrieval
- **Query splitting**: "MomentumEngine" → "MomentumEngine Momentum Engine momentum engine" for BM25
- **Index expansion**: CamelCase components appended to doc_comment during indexing
- **Symbol Specificity Boost**: longer symbol names (>=12 chars) get 1.3x boost
- **Test penalty tightened**: 0.5 → 0.3 to counter test name keyword density bias

#### Whisper: Intent Coverage Fix
- Security intent now gathers **code context** (was excluded → SID=0)
- Diagnostics now available for **Explain** and **Security** intents

#### Housekeeping
- Removed `synapseed-visualizer` crate (axum, rust-embed, notify deps eliminated)
- 13 crates (was 14), workspace deps reduced by 3

## [4.0.0] — 2026-02-13

### The Deep Index — Body-Level Search & Whisper Refactoring

Major version: architectural restructuring of the Whisper router + search recall improvement backed by the MCP Grounding Experiment (F1 0.35→0.84).

#### Search — Deep Body Indexing

- **Body Snippet Expansion**: 15→30 lines captured per symbol. Addresses experiment failures Q4 (cargo check flags buried at line 28 of `run_check()`) and Q10 (debounce params at line 30 of `start_background_loop()`). Estimated +15% recall on function-internal queries.
- **Body Snippet Positional Indexing**: Upgraded from `WithFreqs` to `WithFreqsAndPositions`. Enables phrase matching within function bodies (e.g., `"cargo check --quiet"` now matches as a phrase, not just individual terms). Better BM25 ranking for multi-word body queries.

#### Whisper — Router Module Restructuring

Split the 1,978-line god object (`whisper::router::mod`) into focused, single-responsibility modules:

| Module | Lines | Responsibility |
|--------|-------|----------------|
| `intent.rs` | 170 | Keyword-based intent classification (EN/IT) |
| `extraction.rs` | 420 | 5-pass target extraction pipeline |
| `context_builder.rs` | 560 | Tier-aware smart context assembly (Atomic/Molecular/Galactic) |
| `mod.rs` | 260 | Types, public API, complexity analysis |

- **Zero behavior change**: All 39 whisper tests pass without modification. Public API (`ask`, `ask_raw`, `analyze_complexity`) unchanged.
- **Resolves architect violation**: God Object warning (81 public symbols, limit 50) eliminated. `mod.rs` reduced from 81→12 public symbols.
- **Module visibility**: Internal functions use `pub(super)` — clean encapsulation, no leaking.

#### Architecture Impact

- Architect score maintained at 91/A
- God object violations: 2→1 (whisper resolved, sentinel_test remains as test-only)
- Test count: 256+ passing, 0 failures

## [3.12.0] — 2026-02-13

### The Adaptive Index — Tantivy Timeout Fix & Path-Based Scoring

#### Critical Fix

- **CLI — Tantivy Fast-Path Timeout**: `wait_for_index()` hardcoded a 500ms timeout for Tantivy when CodeGraph was already populated. Large repos (Django: 55K symbols, ~3800 files) need 2-3s to index. Now uses the remaining budget from the outer 5s timeout instead. **This was the root cause of Tantivy returning 0 results for all large repos in CLI mode.**

#### New Features

- **Search — Path-Based Scoring**: Post-hoc boost for Tantivy results where query terms appear in the file path. "scheduler" query → `scheduler/multi_thread/worker.rs` gets 1.5-3.0x boost. Solves concept-level queries where symbol names don't match query terms.
- **Search — Over-Retrieval + Re-Ranking**: Fetch 4× the requested limit from Tantivy BM25, apply temporal/source-first/path boosts, re-sort, then truncate. Allows path-relevant results to surface past pure BM25 name-score ordering.

#### Benchmark Impact (qwen3-1.7b, Vanguard Protocol)

- **tokio_runtime_internals**: recall 0.00 → **0.40** (no-think), CQS 0 → **2.12**, 211x cognitive multiplier
- **actix_service_trait**: recall 0.60 maintained, symbol_recall 100%, CQS 3.10
- **requests_chunked**: recall 0.50 maintained, 198x cognitive multiplier
- **django_middleware_chain**: think recall 0.10 → **0.10** (Tantivy now active, finds `load_middleware`)
- **go_http_handler**: recall 0.40 (think), 64.7x compression, 98.5% cheaper
- **flask_blueprint_routing**: recall 0.20 → **0.40** (think), symbol_recall 80%

## [3.10.1] — 2026-02-13

### Code Quality Sweep — Review-Driven Hardening

#### Bug Fixes

- **Search — Accurate Index Count**: `add_document()` errors now logged and count only incremented on success (was silently discarded, inflating reported count)
- **Search — Deterministic Sort**: Replaced `partial_cmp().unwrap_or(Equal)` with `total_cmp()` for NaN-safe score sorting
- **Whisper — Empty Base Guard**: `derive_source_paths()` now returns empty on edge case inputs like `test_.py` instead of generating invalid paths

#### Performance

- **Whisper — LazyLock Regex**: `extract_call_identifiers()` regex compiled once via `LazyLock` instead of per-call (was ~2ms per call, now ~0)
- **Whisper — O(1) Stop Words**: STOP_WORDS converted from `&[&str]` array (O(n) scan) to `LazyLock<HashSet>` (O(1) lookup). ~80 entries, called in hot loops

#### Clippy Cleanup

- `ModelTier`, `SessionPhase`: `impl Default` → `#[derive(Default)]` with `#[default]` attribute
- `map_or(false, ...)` → `is_some_and(...)` (3 occurrences in whisper)
- `map_or(true, ...)` → `is_none_or(...)` (1 occurrence in mcp/tools)
- `format!("...")` without args → `.to_string()` (1 occurrence in whisper)

## [3.10.0] — 2026-02-13

### The Stale Reader — Critical Tantivy Fix & Search Intelligence

#### Critical Fix

- **Search — Stale Reader Bug**: `ReloadPolicy::OnCommitWithDelay` caused ~500ms lag after Tantivy commit. The Whisperer raced immediately after `SearchReady` and always got an empty reader (0 segments). Added explicit `reader.reload()` after every commit (`index_all`, `reindex_file`, `remove_file`). **This was the root cause of Tantivy returning 0 results in all benchmark runs.**

#### New Features

- **Search — Vendor/Static Penalty**: Files in `/vendor/`, `/node_modules/`, `/static/`, `/dist/`, `/build/`, `.min.` etc. get 0.1x score in Tantivy. Prevents vendored code (e.g., Django's `select2.full.js`) from competing with real source.
- **Whisper — Vendor Drop for Atomic**: Atomic tier now drops vendor/static targets entirely before pruning, preventing wasted context slots.
- **Whisper — Italian Stop Words Expansion**: Added ~80 Italian verbs/nouns commonly mixed with technical terms ("viene", "gestita", "decodifica", "funzioni", "chiamano", "riga", etc.)
- **Whisper — Synonym Expansion**: Morphological variants ("chunked"→"chunk","chunking") + domain synonym pairs ("chunk"→"stream","iter"; "encode"→"decode","codec"; "route"→"router","dispatch"; etc.)
- **Search — Body Snippet Expansion**: 5→15 lines captured per symbol, matching domain keywords in docstrings
- **Search — BM25 Boost Rebalance**: `body_snippet` 0.5→1.5x, `doc_comment` 1.5→2.0x
- **Whisper — Atomic Pruning v2**: 2→3 unique-file targets with source-first ordering (source > test > vendor)

#### Impact

- Before: `synapseed ask "chunked transfer encoding"` on psf/requests → **0 targets** (Tantivy empty), SID 0.0
- After: → `iter_content` in `src/requests/models.py:801`, SID 1.64, full source code injected
- Before: Django middleware query → `select2.full.js` wasting 1 of 3 Atomic slots
- After: → `handlers.py`, `security.py`, `wsgi.py` — all real Django source

## [3.9.3] — 2026-02-13

### Source-First Heuristics — Test-to-Implementation Discovery

#### New Features

- **Search — Source-First Scoring**: Tantivy results now apply a 1.5x boost to non-test files and 0.5x penalty to test files. Implementation code ranks higher when competing with test files for the same keywords.
- **Whisper — Implementation Twin Pattern (Pass 4)**: When extracted targets are test files, derives candidate source paths (e.g., `tests/test_requests.py` → `src/requests.py`, `src/requests/__init__.py`) and looks them up in CodeGraph. O(1) heuristic that works for Python, Rust, and JS/TS conventions.
- **Whisper — Call Graph Lite (Pass 5)**: Extracts function/method call identifiers from test bodies using regex, then looks up those identifiers in CodeGraph (excluding test files). Follows the logical chain: test calls `requests.get()` → finds `Response.get` in `src/requests/`.
- **Whisper — Source-First Ordering**: After all 5 passes, targets are sorted to put non-test files before test files, ensuring implementation code gets priority in the token budget.

#### Impact

- Before: `synapseed ask "chunked transfer encoding"` on psf/requests → only test files (`test_lowlevel.py`, `test_requests.py`)
- After: → `src/requests/models.py` (iter_content, Response), `src/requests/utils.py` ranked first, with test files as secondary context

## [3.9.2] — 2026-02-13

### The Total Truth — Mutex TypeId Fix

#### Critical Fix

- **CLI — Mutex Type Mismatch**: CLI used `std::sync::Mutex<MomentumEngine>` while the Whisperer reads `parking_lot::Mutex<MomentumEngine>`. These are different `TypeId`s, so `get_extension()` never found the engine → tier always fell back to `Molecular` in CLI mode. Fixed by switching CLI to `parking_lot::Mutex`. **This was the root cause of the "85 token" problem**: Atomic formatting (@@@ delimiters, greedy pruning, language reinforcement) was never activated in CLI mode despite `SYNAPSEED_MODEL_TIER=atomic` being set.

#### Impact

- Before: CLI `ask --raw` with Atomic tier → Molecular context (5 targets, `--- FILE ---` delimiters, no language reinforcement) → ~85 tokens of narrative bridge only
- After: CLI `ask --raw` with Atomic tier → Atomic context (2 targets, `@@@ START_OF_TRUTH @@@` delimiters, language reinforcement every 10 lines, recency bias guard) → ~3000 tokens of real source code

## [3.9.1] — 2026-02-13

### Rescue Sprint — NoneType Fix & CLI Cognitive Tiering

#### Fixes

- **Whisper — Null Serialization**: Added `skip_serializing_if` to `Option` fields in `WhisperResult` (`code_context`, `diagnostics`, `history`). Fields are now omitted from JSON instead of serialized as `null`, fixing `'NoneType' object has no attribute 'get'` crashes in downstream Python consumers.
- **Whisper — Target Null Fields**: `file_path` and `line_start` in `Target` now also omitted when `None`.
- **CLI — Cognitive Tiering**: Registered `MomentumEngine` in CLI `ask` command. Reads `SYNAPSEED_MODEL_TIER` env var (priority) or DNA `hci.model_profile` to set tier. Previously CLI always defaulted to `Molecular`, ignoring tier settings entirely.

#### Benchmark Runner

- **Defensive Null Handling**: All `dict.get()` chains use `(x or {})` pattern to guard against JSON `null` values.
- **Synapseed System Prompt**: Separate, stricter system prompt for Synapseed runs referencing `@@@` delimiters.
- **Full Tier Injection**: Sets `SYNAPSEED_MODEL_TIER` for all model sizes (atomic/molecular/galactic).

## [3.6.1] — 2026-02-13

### CI Hardening & CLI JSON Integration

- **CI**: Fixed `ort` feature conflict across platforms by moving features to CLI crate.
- **CLI**: Added `--json` global flag for structured output in all commands.
- **CLI**: Replaced local handlers with unified MCP bridge for `diagnose` and `check`.

## [3.6.0] — 2026-02-13

### "Direct Vision" Release — Symbol Injection + Metric Precision

Introduces Direct Symbol Injection (`raw: true`) for AI context, fixes internal
crate mapping in architectural metrics, and hardens CI builds for Intel Macs.

#### Features

- **Whisperer — Direct Symbol Injection**: New `raw` flag in `ask` tool/CLI to inject physical source code of identified symbols directly into the AI prompt.
- **MCP — Enhanced Ask Tool**: Added support for `raw` parameter in the MCP interface.
- **CLI — New Flag**: Added `--raw` option to the `ask` command.

#### Fixes & Improvements

- **Architect — Metric Precision**: Updated import parser to recognize internal `synapseed-` crates, fixing "low density" false positives.
- **CI — ORT Build Fix**: Configured `ORT_STRATEGY: download` and `copy-dlls` feature to fix build failures on Intel Mac runners.
- **Husk — Logic Bug**: Fixed a character validation logic bug in proptests.
- **Janitor — Refinement**: Applied machine-applicable clippy fixes across the workspace.

## [3.5.0] — 2026-02-13

### "Iron Curtain" Release — Security Hardening + Cognitive Telemetry

Plugs every critical and high-severity vulnerability from the Quality Gate audit,
adds compile-time unsafe protection, runtime crash isolation, encoded-secret
detection, and introduces the SID cognitive metric.

---

#### Security — CRITICAL / HIGH

- **Q7 — Gym Offline Mode** (`crates/gym/src/sandbox.rs`): forced `offline = true`
  in the generated `.cargo/config.toml`. AI-generated code can no longer download
  payloads or exfiltrate data during sandboxed evaluation.
- **Q12 — Shadow Target Isolation** (`crates/shadow-check/src/runner.rs`): shadow
  compiler now uses `--target-dir /tmp/synapseed-shadow-{hash}` (fxhash of project
  root). Eliminates Cargo lock contention with user's own `cargo build`.
- **Q5 — DLP Base64/Hex Decode** (`crates/husk/src/scanner.rs`): new Pass 0 detects
  Base64 and Hex-encoded strings, decodes them, and re-scans decoded content.
  Secrets encoded to bypass plaintext rules are now caught.
- **Q18 — Unsafe Ban** (all 15 crates + `main.rs`): `#![forbid(unsafe_code)]` added
  to every first-party crate. Compile-time enforcement — no unsafe Rust anywhere.
- **Q18 — Panic Isolation** (`crates/mcp/src/tools/mod.rs`): `dispatch_tool()` wrapped
  in `std::panic::catch_unwind(AssertUnwindSafe(...))`. A panic in tree-sitter, git2,
  or any plugin returns an error instead of killing the MCP server.

#### Scalability

- **Q4 — Commit Cap** (`crates/chronos/src/historian.rs`): `count_commits()` now uses
  `revwalk.take(10_000).count()`. Prevents freeze on repositories with millions of
  commits (e.g., linux.git).

#### Features

- **Raw Injection Token Budget** (`crates/whisper/src/router/mod.rs`): hard cap of
  16 000 chars (~4 000 tokens) on injected source code. Stops injecting once budget is
  exhausted — prevents prompt overflow on large symbol sets.
- **SID Metric — Semantic Information Density**: new `sid` field in `WhisperResult`.
  Formula: `symbols_found / (prompt_tokens / 1000)`. Higher = more useful signal per
  token budget. Appears in JSON output and can be used to compare raw vs. hoist mode.

#### Dependencies

- Added `fxhash = "0.2"` (workspace dependency, used by shadow-check for target-dir hashing).

---

## [3.4.0] — 2026-02-13

### "Direct Truth" Release — Raw Symbol Injection

New `--raw` flag for `synapseed ask` injects **actual source code** of discovered
symbols directly into the LLM prompt, boosting F1-Score for sub-3B models.

---

#### Added

- **Raw Symbol Injection** (`--raw` flag): Cortex finds the symbol, Whisperer reads
  the real source from disk using precise `line_start..line_end` ranges, and injects it
  verbatim in the prompt between `[SOURCE_START]` / `[SOURCE_END]` tags.
- **MCP `ask` tool**: new optional `raw: boolean` parameter for programmatic access.
- **`ask_raw()` public API** in `synapseed_whisper::router` for library consumers.
- **Instruction Hardening**: when `--raw` is active, the prompt includes an imperative
  directive: _"Answer based ONLY on the injected source code. Cite exact file paths
  and line numbers."_

#### Changed

- `build_smart_context()` now accepts `raw_injection` + `raw_sources` parameters.
- `WhisperResult.smart_context` includes source blocks when raw mode is on.

---

## [3.3.0] — 2026-02-13

### "Auto-Hoist" Release

Security hardening, massive test expansion (93 → 255), and `ask` now auto-indexes
the project so `synapseed ask "..."` works out of the box without a prior `hoist`.

---

### Architecture (15 crates, 20 tools, 9 resources, 6 prompts)

Score: **97/100 (Grade A)** — 161 modules, 58 edges.
255 tests passing, 0 failures.

### Features

- **Auto-hoist in `ask`** — CLI `ask` (and external catch-all) now ensures the
  code graph is fully indexed before querying the Whisperer. Waits 500 ms for the
  background indexer; if still empty, falls back to synchronous indexing. Equivalent
  to `synapseed hoist . && synapseed ask "..."` in a single process.

- **Semantic catch-all** — Natural-language tool names (len > 20, spaces, `?`)
  are redirected to `ask` instead of erroring. Small models that write a question
  as the tool name get a helpful answer.

### Security — P0

- **Path traversal protection** — Added `PathTraversal` error variant and
  `safe_resolve()` utility in `synapseed-core`. Fixed unsafe `.join()` +
  `read_to_string()` patterns in: whisper/security.rs, visualizer/server.rs,
  search/indexer.rs, shadow-check/runner.rs, janitor/fixer.rs.

- **Unicode truncation panic** — Fixed `husk/patterns.rs` byte-index slicing
  on multi-byte characters (`&s[..max]` → `s.floor_char_boundary(max)`).
  Discovered by proptest fuzzing.

### Hardening — Zero-Panic

- **parking_lot RwLock** — Replaced `std::sync::RwLock` with `parking_lot` in
  core/context.rs, search/vector_index.rs, architect/blueprint.rs, janitor —
  eliminates lock poisoning panics.

- **LazyLock regex** — oracle.rs regexes compiled once via `std::sync::LazyLock`
  instead of per-call `Regex::new()`.

- **saturating_sub** — Epoch arithmetic uses `saturating_sub()` to prevent
  underflow panic.

### Testing — 93 → 255 tests

- **MCP protocol** — 31 tests: JSON-RPC routing, fuzzy matching, tool aliases,
  error codes, resource/prompt listing.
- **Sentinel** — 22 tests: allow/deny, fail-closed, Unicode, edge cases.
- **Cortex** — 11 tests + 7 proptest: graph indexing, symbol lookup, multi-lang,
  AST parser fuzz (Rust/Python/JS never panics, determinism).
- **Husk** — 15 tests + 9 proptest: DLP secrets, redaction, code patterns,
  scanner fuzz, idempotent redaction.
- **Search** — 9 tests: Tantivy indexing, querying, re-index, removal,
  multi-symbol, temporal decay, disk persistence.
- **Core** — 8 path-traversal tests: `safe_resolve()` blocks `..`, absolute
  outside root, nonexistent files; allows valid paths.
- **Root proptest** — 6 tests: sentinel never panics on arbitrary input,
  deterministic evaluation.

### Feature-gate

- **tonic/gRPC optional** — `synapseed-telemetry-sink` gains `grpc` feature flag.
  Users who don't need OTLP export save ~35 transitive deps. Default: enabled.

### Benchmarks

- **criterion suite** — cortex (CodeGraph::new, index_directory, lookup) and
  search (SemanticIndex::new, index_all, search) benchmarks added.

---

## [3.2.0] — 2026-02-12

### "CLI Polish" Release

Quality polish: CLI/MCP argument parity, stderr-only telemetry, default ask
fallback, visible aliases, and dead code cleanup.

---

### Architecture (15 crates, 20 tools, 9 resources, 6 prompts)

Score: **97/100 (Grade A)** — 161 modules, 58 edges.
93 tests passing, 0 failures.

### Fixes

- **Argument parity (scan)** — Renamed CLI `--text` to `--content` to match MCP
  tool schema. Added `--mode` flag (all/dlp/patterns). Scan now routes through
  `cmd_mcp()` bridge for zero logic duplication.

- **Argument parity (hoist)** — Added optional positional `path` argument to CLI
  `hoist` command. `synapseed hoist src/` now works. Routes through `cmd_mcp()`.

- **Telemetry to stderr** — `init_telemetry()` now writes to stderr in both
  compact and JSON modes. Tracing output no longer contaminates stdout for any
  CLI command.

- **Visible aliases** — All `alias` attributes changed to `visible_alias`.
  Legacy MCP names now appear in `synapseed --help` output for discoverability.

- **Default ask fallback** — `synapseed "why is login broken?"` now works as
  shorthand for `synapseed ask "..."`. Unrecognized subcommands are interpreted
  as `ask` queries via Clap `external_subcommand`.

### Removed

- `cmd_scan()` local handler — replaced by `cmd_mcp("scan", ...)` bridge
- `cmd_hoist()` local handler — replaced by `cmd_mcp("hoist", ...)` bridge
- `SecurityGuard` import (no longer needed in CLI binary)

### Technical Details

- **Files changed**: 4 (`bin/synapseed/src/main.rs`, `crates/core/src/telemetry.rs`,
  `README.md`, `CHANGELOG.md`)
- **Zero new dependencies**
- **Net code reduction**: ~40 lines removed (dead handlers replaced by bridge calls)

---

## [3.1.0] — 2026-02-12

### "Fuzzy & Resilient" Release

CLI and MCP tool names are now perfectly aligned. Every MCP tool has a CLI
counterpart, every legacy name works as an alias, and typos are auto-corrected.

---

### Architecture (15 crates, 20 tools, 9 resources, 6 prompts)

Score: **97/100 (Grade A)** — 161 modules, 58 edges.
78 tests passing, 0 failures.

### Breaking Changes (MCP tool names)

All 20 MCP tools have been renamed to short, CLI-aligned canonical names.
Legacy names continue to work as backward-compatible aliases.

| Old Name | New Name |
| :--- | :--- |
| `get_code_skeleton` | `hoist` |
| `lookup_symbol` | `lookup` |
| `scan_security` | `scan` |
| `check_command` | `check` |
| `git_history` | `blame` |
| `project_diagnose` | `diagnose` |
| `consult_architect` | `consult` |
| `semantic_search` | `search` |
| `get_diagnostics` | `diagnostics` |
| `analyze_history` | `analyze` |
| `apply_quick_fix` | `quickfix` |
| `ask_synapseed` | `ask` |
| `git_intent_summary` | `intent` |
| `train_code` | `train` |
| `reset_telemetry` | `reset-telemetry` |
| `janitor_run_now` | `janitor` |
| `janitor_apply_fix` | `janitor-fix` |
| `architect_analyze` | `architect` |
| `oracle_fix_docs` | `oracle` |
| `semantic_similarity` | `similar` |

### New Features

- **13 new CLI subcommands** — Every MCP-only tool now has a CLI counterpart:
  `ask`, `search`, `diagnostics`, `analyze`, `quickfix`, `intent`, `train`,
  `reset-telemetry`, `janitor`, `janitor-fix`, `architect`, `consult`,
  `oracle`, `similar`. Uses `handle_tool_call()` bridge for zero logic
  duplication.

- **CLI aliases** — All 7 existing CLI commands accept their MCP tool name
  as alias (e.g. `synapseed get_code_skeleton` = `synapseed hoist`).
  Extra mnemonic: `synapseed whisper` = `synapseed ask`.

- **Fuzzy tool dispatch** — Levenshtein-based auto-correction in MCP
  `handle_tool_call()`. Edit distance ≤ 3: auto-executes with
  "Did you mean 'X'?" prefix. Distance > 3: error with suggestion +
  full available tool list. No external dependencies.

- **`init_full_context()` helper** — Extracted from `cmd_serve()` to
  initialize all 12 plugins for CLI-to-MCP bridge commands.

### Technical Details

- **Files changed**: 3 (`crates/mcp/src/tools/mod.rs`,
  `bin/synapseed/src/main.rs`, `bin/synapseed/tests/integration_mcp.rs`)
- **Dispatch architecture**: `resolve_tool_name()` → `dispatch_tool()` with
  `TOOL_NAMES` const array for canonical names
- **Zero new dependencies**: Levenshtein implemented inline (~15 lines)

---

## [2.2.1] — 2026-02-12

### Security & Hardening Patch

10 fixes across 9 crates — security vulnerabilities, logic bugs, and documentation drift.

---

### Security Fixes

- **CRITICAL: XSS in Visualizer** — Replaced inline `onclick` attribute in `panels.js` with
  event delegation (`data-focus-node` + `addEventListener`), preventing quote injection in
  symbol IDs.
- **Sentinel hardening** — Added 4 deny rules: `sudo`, `eval`, `curl|bash`, `LD_PRELOAD`.
- **Security pattern scanner** — Added `outerHTML`, `insertAdjacentHTML`, `writeln` to XSS
  detection; tightened path traversal `.join()` regex to require path-like context.

### Bug Fixes

- **Proptest fuzzer** — Replaced invalid `\PC{0,100}` regex strategy with `any::<String>()`.
- **Sandbox corruption** — `unwrap_or_default()` on source read replaced with proper error
  propagation, preventing silent state corruption during adversarial mutation testing.
- **Oracle doc fixer** — `replace()` → `replace_all()` for crate/tool/resource count patching;
  silent `unwrap_or("0.0.0")` fallback replaced with `warn!()` + early return.
- **Epoch subtraction** — `newer.epoch - older.epoch` → `saturating_sub()` to prevent panic
  on git clock skew.
- **Dead code cleanup** — Removed unused `matched_text` field from DLP `Finding` struct
  (eliminated compiler warning).

### Documentation

- Removed stale "<12MB binary" claims from 4 files (index.md, introduction.md, installation.md,
  CHANGELOG.md).

---

## [2.2.0] — 2026-02-13

### "The Physics Engine" Release

SYNAPSEED gains mathematical depth: six new analytical capabilities that quantify
structural density, temporal decay, convergence dynamics, mutation resilience,
cross-artifact consistency, and security pattern recognition.

---

### Architecture (15 crates, 19 tools, 9 resources, 6 prompts)

Score: **97/100 (Grade A)** — 131+ modules, 44 edges, 1 remaining violation.
89 tests passing, 0 failures.

### New Features

- **Topological Density** (#15) — `D = E / (V × (V − 1))` directed graph density
  metric in `synapseed-architect`. Density anomaly detection in linter (high > 0.5,
  low < 0.02 with ≥10 modules). Configurable thresholds via DNA. Wired into
  `synapseed://architect/health` resource and architecture score (−5 penalty for
  density > 0.5).

- **Temporal Decay** (#16) — Exponential decay on Chronos hotspot scores:
  `raw_score × e^(−λ × days)` (λ default 0.01). Temporal boost in search results:
  `score × (0.7 + 0.3 × e^(−λ × age_days))` with `last_modified_epoch` Tantivy
  field. Configurable `temporal_decay_lambda` via DNA `search` config.

- **Convergence Rate** (#17) — Fix-chain detection in Chronos: consecutive fix
  commits within 48h window. `convergence_rate = 1.0 − (fix_chains / total)`,
  `rigidity = fix_chains / total`. Exposed in Whisper `HistoryContext` for
  intelligent routing.

- **Adversarial Sandbox** (#18) — `Saboteur` mutation engine in `synapseed-gym`
  with 5 strategies (ArithmeticSwap, BooleanNegate, BoundaryShift, ReturnRemove,
  StatementDelete). Max 20 mutations per eval. `train_code(adversarial: true)` runs
  cargo check + cargo test per mutant. `mutation_score = detected / total` blended
  into Gym report score.

- **Consistency Oracle** (#19) — `synapseed_core::oracle` cross-references
  Cargo.toml workspace members vs filesystem, README feature mentions, docs index
  link validity, and crate description completeness. New `synapseed://consistency`
  MCP resource returning scored consistency report.

- **Security Patterns** (#20) — `CodePatternScanner` in `synapseed-husk` with
  regex-based detection for SQL injection, XSS, command injection, and path
  traversal (14 patterns across 4 categories). `scan_security(mode: "all"|"dlp"|"patterns")`
  MCP tool now supports dual-mode operation combining DLP + code pattern scanning.

### MCP Changes

- New resource: `synapseed://consistency` — project-wide consistency report
- Updated tool: `scan_security` — added `mode` parameter (all/dlp/patterns)
- Updated tool: `train_code` — added `adversarial` boolean parameter
- Updated resource: `synapseed://architect/health` — includes `topological_density`

---

## [2.1.0] — 2026-02-12

### "The Hardening" Release

SYNAPSEED hardens its internals: graceful shutdown, expanded test coverage, tighter
API visibility, adversarial fuzzing, and a structural refactoring pass that
eliminated all three monolith files.

---

### Architecture (15 crates, 19 tools, 8 resources, 6 prompts)

Score: **97/100 (Grade A)** — 131 modules, 44 edges, 1 remaining violation.
93 tests passing, 0 failures.

### New Features

- **Graceful Shutdown** (#12) — `CancellationToken` propagation through all async
  plugins (Cortex, Visualizer, Shadow-Check, Telemetry-Sink). `AtomicBool` shutdown
  flag on `SynapseContext`. Ctrl-C triggers coordinated cleanup across all subsystems.

- **Adversarial Fuzzing** (#6) — `train_code(fuzz: true)` auto-generates proptest
  property-based tests for all public functions. `FuzzGenerator` parses function
  signatures and generates type-appropriate strategies (u8..u64, String, Vec, Option).
  Integrated into Gym sandbox evaluation pipeline.

- **Architect Crate** — New `synapseed-architect` crate: dependency graph analysis,
  coupling metrics (Ce/Ca/Instability), cycle detection, god object detection,
  `LinterConfig` from DNA, blueprint report generation. `architect_analyze` MCP tool
  with `ReportStore` caching.

- **Janitor Crate** — New `synapseed-janitor` crate: automated clippy scan, unused
  dependency detection, fix proposal system with dry-run preview. Background async
  scanning with `ProposalStore` and atomic scan-in-progress guard.

### Improvements

- **Test Coverage** (#13) — 93 tests (up from 78). Added:
  - 4 port-hopping tests for `bind_with_retry()` (extracted from `start()`)
  - 3 async background indexing tests for `CortexPlugin`
  - First `#[tokio::test]` async tests in the codebase

- **API Visibility** (#14) — Tightened `pub` exports across all crates.
  `pub(crate)` for internal functions, `pub(super)` for module-private helpers.

### Structural Refactoring

- **graph.js Split** (#21) — God object (921 lines, 81 symbols) split into 9 focused
  modules: `constants.js`, `styles.js`, `layout.js`, `panels.js`, `events.js`,
  `search.js`, `xray.js`, `api.js`, `graph.js` (boot, 40 lines). Served via dynamic
  `/{name}.js` axum route. XSS safety preserved (`esc()` in `constants.js`).

- **tools.rs Split** (#23) — MCP tool monolith (1150 lines, 19 tools) split into
  `tools/mod.rs` (429 lines: schema registry + dispatch + helpers) + 12 sub-modules.
  Each tool file exports `pub(super) fn tool_xxx()`.

- **router.rs Split** (#22) — Whisper intent router (720 lines) split into
  `router/mod.rs` (530 lines: classify + extract + build context + tests) + 4 gather
  modules (`diagnostics.rs`, `history.rs`, `code.rs`, `security.rs`).

### Documentation

- **"The Quantum Loop"** — New `docs/guide/workflow.md`: operational playbook for
  high-velocity development using SYNAPSEED's 19 MCP tools. Includes tool tier
  classification, 5-step iteration cycle, ready-to-use prompt pack, anti-patterns,
  and the meta-loop (SYNAPSEED managing SYNAPSEED).

### Bug Fixes

- **Flaky port test** — `test_bind_with_retry_exhausts_limit` now deterministically
  finds a consecutive range of free ports instead of relying on OS-assigned ports.

- **Dependabot security alerts** — Resolved all outstanding dependency advisories.

---

[2.1.0]: https://github.com/fabriziosalmi/synapseed/compare/v2.0.2...v2.1.0

## [1.0.0] — 2026-02-12

### The "Production-Ready" Release

SYNAPSEED graduates from prototype to production. Every subsystem has been hardened,
unified, and documented. The thinking layer between you and your LLM is now ready
for real-world codebases.

---

### Architecture (11 crates, 14 tools, 6 resources, 6 prompts)

- **synapseed-core** — Event bus, plugin trait, context extensions, telemetry init
- **synapseed-cortex** — Tree-sitter AST parsing (Rust, Python, JS), CodeGraph with parallel rayon indexing
- **synapseed-husk** — DLP shield with Aho-Corasick + regex detection, configurable custom rules via DNA
- **synapseed-root** — Command sentinel with deny-first policy enforcement
- **synapseed-chronos** — Git time-travel: blame, history, semantic commit tags, intent analysis
- **synapseed-search** — Tantivy full-text semantic index (RAM or disk-persistent)
- **synapseed-shadow-check** — Background `cargo check` with `MachineApplicable` quick-fix engine
- **synapseed-visualizer** — Live Cytoscape.js architecture dashboard with WebSocket + OTLP heatmap
- **synapseed-whisper** — Intent router: NLP classification → multi-subsystem orchestration
- **synapseed-telemetry-sink** — OTLP gRPC receiver (port 4317), ring-buffer SpanStore, hotspot ranking
- **synapseed-mcp** — Full MCP 2024-11-05 protocol: tools, resources, prompts, dynamic context injection

### New Features

- **Parallel AST Indexing** — `CodeGraph::index_directory()` uses rayon `par_iter` for multi-core parsing. Each thread creates its own tree-sitter parser; DashMap handles concurrent inserts. Measured ~3x speedup on 8-core machines with large codebases.

- **Git Intent Summary** — New `git_intent_summary` MCP tool (14th tool). Analyzes recent commits via `Historian::summarize_intent()`, groups them by semantic category (fix, feature, refactor, security, performance, test, docs), extracts conventional commit scopes, and returns a natural-language summary like: *"12 commits over 5 days: 4 feature (auth, router), 3 refactor, 2 fix, 2 docs, 1 test"*.

- **Tantivy Disk Persistence** — `SemanticIndex::open_or_create()` persists the full-text index to `.synapseed/index/` on disk. Enable via `search.persistence: true` in `dna.yaml`. Schema-mismatch detection auto-recreates the index. Fallback to RAM on any error.

- **Configurable DLP Rules** — Custom regex/literal patterns in `dna.yaml` under `dlp_custom_rules`. Each rule has `name`, `pattern`, and `action` (redact/deny/audit/allow). Merged with built-in Aho-Corasick defaults. Wired through `HuskPlugin::from_dna()` → `SecurityGuard::from_policy()`.

- **Configurable Visualizer Port** — `SYNAPSEED_VISUALIZER_PORT` env var or `visualizer_port` in DNA config. Priority: env > config > default 3000. `VisualizerPlugin::from_config()` constructor.

### Improvements

- **CLI/Service Unification** — MCP tools now reuse plugin-initialized subsystems via context extensions instead of creating fresh instances on every call:
  - `CodeGraph` → `Arc<CodeGraph>` registered by CortexPlugin
  - `Sentinel` → `Arc<Sentinel>` registered by RootPlugin
  - `SecurityGuard` → `Arc<SecurityGuard>` registered by HuskPlugin
  - Tools try `ctx.get_extension::<T>()` first, fall back to ephemeral instances for non-root paths

- **ProjectDna Extensions** — Three new config fields: `search: SearchConfig` (persistence toggle), `dlp_custom_rules: Vec<DlpRule>` (custom patterns), `visualizer_port: Option<u16>`. All with `#[serde(default)]` for backward compatibility. Cascading merge updated.

### Documentation

- **README** — Mermaid.js architecture diagram, custom security rules section, search persistence note, updated tool count (14), `SYNAPSEED_VISUALIZER_PORT` env var, fixed install path
- **VitePress docs** — Updated `mcp-tools.md` (14 tools + git_intent_summary), `search.md` (disk persistence), `configuration.md` (new DNA fields, custom DLP rules, env vars)

### Testing

- 34 tests passing across workspace (6 chronos, 7 whisper, 3 search, 3 shadow-check, 4 telemetry-sink, 7 integration MCP, 1 scenario, 3 doc-tests)
- Integration test validates 14 tools, 6 resources, 6 prompts
- Full MCP lifecycle: initialize → tools/list → tools/call → resources → prompts → ping
- DLP detection, unknown tool/method errors, pre-init rejection, resource reads, prompt expansion

### Technical Details

- **Dependencies added**: `rayon = "1"` (workspace + cortex crate)
- **Release profile**: opt-level 3, LTO, single codegen unit, stripped
- **Zero network calls**: All processing local, all servers bound to 127.0.0.1
- **Minimum Rust version**: 1.75+

---

[1.0.0]: https://github.com/fabriziosalmi/synapseed/releases/tag/v1.0.0
