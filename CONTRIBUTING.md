# Contributing to SYNAPSEED

Thank you for your interest in contributing! This guide will help you get started, whether you're fixing a typo or adding a major feature.

## 🚀 Quick Start for New Contributors

**Never contributed to open source before?** No problem!

1. Check out [GOOD_FIRST_ISSUES.md](GOOD_FIRST_ISSUES.md) for beginner-friendly tasks
2. Pick something that interests you
3. Follow the steps below to submit your changes

**Already familiar with open source?** Jump straight to [Development Workflow](#development-workflow).

---

## Prerequisites

- **Rust** stable toolchain (1.75+) — install via [rustup](https://rustup.rs/)
- **Git** 2.x+
- **Node.js** 18+ (only for visualizer E2E tests)

## Setup

```bash
git clone https://github.com/fabriziosalmi/synapseed.git
cd synapseed
cargo build
```

Verify everything works:

```bash
cargo test --workspace
```

## Project Structure

```
synapseed/
  bin/synapseed/       # CLI binary
  crates/
    core/              # Shared types, context, plugin trait
    cortex/            # AST parser + code graph
    husk/              # DLP scanner + security guard
    root/              # Command sentinel + executor
    chronos/           # Git historian + analyzer
    mcp/               # MCP JSON-RPC server
    search/            # Tantivy full-text search
    visualizer/        # Axum dashboard + WebSocket
    shadow-check/      # Background diagnostics
    whisper/           # Event router
    telemetry-sink/    # OTLP span collector
  tests/visualizer/    # Playwright E2E tests
  docs/                # VitePress documentation
```

## Development Workflow

### 1. Create a branch

```bash
git checkout -b feat/your-feature
```

### 2. Make your changes

Follow the existing code style. The codebase uses:
- `anyhow::Result` in the binary, `thiserror` in libraries
- `DashMap` for concurrent data structures
- `tokio` for async runtime
- Plugin trait pattern (`SynapsePlugin`) for extensibility

### 3. Verify quality

All of the following **must pass** before opening a PR:

```bash
# Format check
cargo fmt --all -- --check

# Lint (zero warnings)
cargo clippy -- -D warnings

# Tests
cargo test --workspace
```

### 4. Run visualizer E2E tests (if you changed UI)

```bash
cd tests/visualizer
npm install
npx playwright install chromium
npx playwright test
```

### 5. Open a Pull Request

- Fork the repository and push your branch
- Open a PR against `main`
- Fill in the PR template with a clear description
- Link any related issues

## Code Standards

### Rust

- **No `unsafe`** unless absolutely required (and documented)
- **No `unwrap()`** in library crates — use `?` or `expect()` with a clear message
- **`unwrap()` on `RwLock`** is acceptable (only panics on poisoned locks)
- Prefer `&Path` over `&PathBuf` in function signatures
- Keep public APIs minimal — only expose what other crates need

### Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add new MCP tool for dependency analysis
fix: handle empty git repos in historian
chore: update tantivy to 0.23
test: add Playwright tests for search filter
docs: update configuration reference
```

### Adding a New Plugin

1. Create a new crate under `crates/`
2. Implement the `SynapsePlugin` trait from `synapseed-core`
3. Register it in `bin/synapseed/src/main.rs` (`cmd_serve`)
4. Add workspace dependency in root `Cargo.toml`
5. Update integration tests in `bin/synapseed/tests/`

## Reporting Issues

- **Bugs**: open a [GitHub issue](https://github.com/fabriziosalmi/synapseed/issues) with reproduction steps
- **Security vulnerabilities**: see [SECURITY.md](SECURITY.md) — do not open public issues
- **Feature requests**: open an issue with the `enhancement` label

## License

By contributing, you agree that your contributions will be licensed under the [Apache License 2.0](LICENSE).
