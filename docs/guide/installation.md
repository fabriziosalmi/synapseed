# Installation

## Prerequisites

- **Rust Toolchain** — 1.75 or later ([rustup.rs](https://rustup.rs/))
- **Git** — Required for Chronos time-travel features
- **C compiler** — Required by tree-sitter (usually pre-installed on macOS/Linux)

## Build from Source

```bash
git clone https://github.com/fabriziosalmi/synapseed.git
cd synapseed
cargo install --path bin/synapseed --force
```

Verify installation:

```bash
synapseed --version
# synapseed 1.0.0
```

## Release Build

For maximum performance (LTO + single codegen unit):

```bash
cargo build --release
# Binary at target/release/synapseed
```

## Development Build

```bash
cargo build
cargo test
```

The workspace contains 15 library crates and 1 binary crate. All tests run with:

```bash
cargo test
# 512 tests across all crates
```

## Platform Support

| Platform | Status |
| :--- | :--- |
| macOS (ARM64) | Fully tested |
| macOS (x86_64) | Supported |
| Linux (x86_64) | Supported |
| Windows | Should work (untested) |

## Workspace Layout

```
synapseed/
├── Cargo.toml              # Workspace root
├── bin/synapseed/          # CLI binary
├── crates/
│   ├── core/               # Shared types, traits, event bus, consistency oracle
│   ├── cortex/             # AST parsing & code graph
│   ├── husk/               # DLP, secret detection & code pattern scanning
│   ├── root/               # Command sandbox & sentinel
│   ├── chronos/            # Git history, temporal decay & convergence
│   ├── search/             # Tantivy semantic search with embeddings
│   ├── shadow-check/       # Background compiler
│   ├── architect/          # Architecture analysis & health scoring
│   ├── gym/                # Code evaluation sandbox & adversarial testing
│   ├── janitor/            # Automated clippy & dependency cleanup
│   ├── bench/              # Reproducible evaluation framework
│   ├── decompiler/         # Binary analysis (ELF/Mach-O/PE)
│   ├── whisper/            # Intent router
│   ├── telemetry-sink/     # OTLP gRPC receiver
│   └── mcp/                # MCP protocol bridge
└── docs/                   # This documentation
```
