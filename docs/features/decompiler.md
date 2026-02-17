# Neural Decompiler

The Neural Decompiler is a specialized module for **binary analysis**. It enables SYNAPSEED to understand compiled binaries (like proprietary dependencies or legacy code) where source code is unavailable.

## Supported Formats

- **ELF** (Linux)
- **Mach-O** (macOS)
- **PE** (Windows)

## Capabilities

The Decompiler performs static analysis on binaries to extract:

1. **Symbols**: Exported functions and global variables.
2. **Strings**: Classified strings (URLs, file paths, SQL queries, error messages).
3. **Call Graph**: Inter-function call relationships.
4. **Behavioral Inference**: Uses heuristics to tag the binary's likely purpose (e.g., `NetworkIO`, `Crypto`, `Database`).

## Usage

### MCP Tool: `analyze_binary`
Analyze any binary file on disk.
```bash
synapseed analyze_binary --path /usr/bin/git
```

### MCP Tool: `explain_dependency`
Analyze a compiled Rust dependency by crate name. Useful if you want to understand what a dependency *really* does in its compiled form.
```bash
synapseed explain_dependency --crate_name "tokio"
```

## How it works

The Decompiler uses the `goblin` crate for parsing executable formats. It does **not** lift assembly to high-level code (C/Rust) yet; instead, it focuses on **behavioral understanding** through symbol and string analysis, providing a high-level summary of what the binary is capable of.
