# Test Coverage

## Current Status

📊 **255 tests passing** (up from 93 in v3.2.0)

Coverage tracking is now enabled via GitHub Actions with `cargo-llvm-cov` and Codecov.

---

## Philosophy

Test count alone is deceptive — 255 tests include many smoke tests. **Line coverage** is the objective baseline.

Current strategy: **report-only** (no CI gate). Once baseline is established, we'll add gates incrementally.

---

## Coverage Roadmap

### Phase 1: Baseline & Report (Current)
**Status:** ✅ Implemented in v3.3.0  
**Goal:** Establish objective coverage measurement

- GitHub Action generates coverage on every push
- Codecov badge in README
- **No CI gate** — report only
- Identify gaps and prioritize P0 gaps (security boundaries)

### Phase 2: Minimum Gate (v4.0.0)
**Status:** Planned  
**Goal:** Enforce minimum acceptable coverage

- **20% gate** — PR blocked below this threshold
- Focus on critical paths:
  - Path traversal validation (`synapseed-core::error::safe_resolve`)
  - DLP scanner redaction (`synapseed-husk`)
  - Command sentinel evaluation (`synapseed-root`)
  - MCP protocol routing (`synapseed-mcp`)

### Phase 3: Production Gate (v5.0.0)
**Status:** Future  
**Goal:** Production-ready coverage

- **40% gate** across workspace
- 60%+ for security-critical crates (husk, root, core)
- 80%+ for public API surfaces

---

## Per-Crate Baselines

Once the first coverage run completes, this table will be populated:

| Crate | LOC | Tests | Coverage | Priority |
|-------|-----|-------|----------|----------|
| `synapseed-core` | ~1900 | 15 | TBD | P0 (security) |
| `synapseed-husk` | ~1400 | 24 | TBD | P0 (DLP) |
| `synapseed-root` | ~800 | 28 | TBD | P0 (sentinel) |
| `synapseed-mcp` | ~1800 | 31 | TBD | P1 (protocol) |
| `synapseed-cortex` | ~1600 | 18 | TBD | P1 (indexing) |
| `synapseed-search` | ~1400 | 9 | TBD | P2 |
| `synapseed-whisper` | ~900 | 0 | TBD | P2 |
| `synapseed-visualizer` | ~1200 | 5 | TBD | P2 |
| Others | ~5000 | 125 | TBD | P3-P4 |

---

## Running Locally

```bash
# Install cargo-llvm-cov
cargo install cargo-llvm-cov

# Generate coverage report
cargo llvm-cov --workspace --lcov --output-path lcov.info

# Generate HTML report (for inspection)
cargo llvm-cov --workspace --html
open target/llvm-cov/html/index.html
```

---

## Exclusions

The following are intentionally excluded from coverage:
- `target/` (build artifacts)
- `tests/` (test code itself)
- `benches/` (benchmarks)
- Generated code (if any)

---

## References

- [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov)
- [Codecov](https://about.codecov.io/)
- Issue [#37](https://github.com/fabriziosalmi/synapseed/issues/37): CI coverage tracking
