# ADR: Keep gym and janitor as separate crates

**Date:** 2025-02-14  
**Status:** Accepted  
**Issue:** #68

## Context

The gym crate (~1,627 LOC) and janitor crate (~1,046 LOC) both deal with
code quality validation:

- **gym**: Isolated sandbox for Rust code eval, mutation testing, proptest
  fuzzing. General-purpose — takes arbitrary source code.
- **janitor**: Clippy warning detection, unused dependency scanning, UUID-
  tracked fix proposals. Project-specific — operates on the workspace.

They share a conceptual theme (code quality) and janitor already depends on
gym for sandbox-based fix validation.

## Decision

**Keep them as separate crates.** Rationale:

1. **Different abstraction levels.** Gym is a general execution sandbox
   (any Rust source → compile/test/bench). Janitor is a project analysis
   tool (workspace → clippy/cargo diagnostics → proposals). Merging would
   conflate an execution engine with a diagnostic scanner.

2. **Dependency asymmetry.** Gym has heavy optional deps (proptest) that
   janitor doesn't need. Merging would either bloat janitor or require
   internal feature gates — adding complexity with no API benefit.

3. **Different change velocity.** Gym's sandbox/eval logic is stable
   (changes ~2x/quarter). Janitor evolves with each clippy release and
   each new proposal workflow. Separate crates keep CI granular.

4. **API already clean.** After restricting gym's module visibility
   (adversarial, fuzzer, sandbox, report, scenario → `pub(crate)`), the
   external API surface dropped from **34 pub items to 6**:
   `Trainer`, `Scenario`, `Report`, `GymError`, `GymPlugin`, `plugin` mod.
   No further consolidation needed.

5. **Janitor→Gym dependency is correct.** Janitor calls `Trainer::evaluate()`
   to sandbox-validate fix proposals before applying them. This is a clean
   consumer relationship, not a sign they should merge.

## Consequences

- Gym stays at `crates/gym/`, janitor at `crates/janitor/`.
- Gym's internal modules (`adversarial`, `fuzzer`, `sandbox`, `report`,
  `scenario`) are `pub(crate)` — hidden from downstream consumers.
- Janitor continues to depend on gym for fix validation.
- Both can be independently feature-gated from MCP if needed.
