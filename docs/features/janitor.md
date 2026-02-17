# The Janitor (Autonomous Maintenance)

The Janitor is an autonomous agent dedicated to **codebase hygiene**. It continuously monitors the project for "low-hanging fruit" — minor issues that accumulate over time but are tedious to fix manually.

## Responsibilities

1. **Clippy Warnings**: Identifies and fixes common Rust lints (style, correctness, complexity, perf).
2. **Unused Dependencies**: Detects dependencies in `Cargo.toml` that are not imported in any source file.
3. **Compiler Warnings**: Monitors standard compiler warnings.

## Workflow

The Janitor operates on a **Safe Proposal** model. It never modifies your code without explicit confirmation.

1. **Scan**: usage `synapseed janitor`
   - Runs `cargo clippy --message-format=json`.
   - Scans `Cargo.toml` and source files for unused dependencies.
   - Generates a list of **Proposals**.

2. **Review**:
   - Each proposal has a unique ID, a description (e.g., "Remove unused import"), and a severity.
   - Proposals are stored in memory resource `synapseed://janitor/proposals`.

3. **Apply**: usage `synapseed janitor-fix`
   - You (or the LLM) invoke `janitor-fix` with a proposal ID.
   - **Dry Run**: By default, it shows a diff of what *would* happen.
   - **Confirm**: Call with `confirm: true` to apply the change.
   - **Verification**: After applying, it runs `cargo check`. If the check fails, it **automatically reverts** the change to ensure the codebase remains compiling.

## Integration

The Janitor is designed to run in the background or on-demand. In **Claude Desktop** or **VS Code**, the Janitor's findings can be presented as a list of actionable tasks.

## Protocol

- **Tool**: `janitor` (scan), `janitor-fix` (apply)
- **Resource**: `synapseed://janitor/proposals` (active proposals)
