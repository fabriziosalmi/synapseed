# Claude Code

Integration with Claude Code (CLI).

## Setup

Add SYNAPSEED as an MCP server in your Claude Code settings:

```json
// ~/.claude/settings.json
{
  "mcpServers": {
    "synapseed": {
      "command": "synapseed",
      "args": ["serve", "--project", "."],
      "env": {
        "RUST_LOG": "warn"
      }
    }
  }
}
```

## Project-Level Configuration

Create a `.mcp.json` in your project root for per-project settings:

```json
{
  "mcpServers": {
    "synapseed": {
      "command": "synapseed",
      "args": ["serve", "--project", "."],
      "env": {
        "RUST_LOG": "warn",
        "SYNAPSEED_SELF_TELEMETRY": "1"
      }
    }
  }
}
```

## Usage

Once configured, Claude Code can use SYNAPSEED tools directly:

```
> Ask Claude: "Use synapseed to explain this project's architecture"
> Ask Claude: "Check if there are any security issues in my code"
> Ask Claude: "What are the compiler errors right now?"
```

## Verification

```bash
# Check that synapseed is accessible
synapseed --version

# Test MCP communication manually
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}' | synapseed serve --project .
```
