# Claude Desktop

Native MCP integration with Claude Desktop.

## Setup

Add this to your Claude Desktop configuration file:

::: code-group

```json [macOS]
// ~/Library/Application Support/Claude/claude_desktop_config.json
{
  "mcpServers": {
    "synapseed": {
      "command": "synapseed",
      "args": ["serve"],
      "env": {
        "RUST_LOG": "info"
      }
    }
  }
}
```

```json [Windows]
// %APPDATA%\Claude\claude_desktop_config.json
{
  "mcpServers": {
    "synapseed": {
      "command": "synapseed.exe",
      "args": ["serve"],
      "env": {
        "RUST_LOG": "info"
      }
    }
  }
}
```

```json [Linux]
// ~/.config/Claude/claude_desktop_config.json
{
  "mcpServers": {
    "synapseed": {
      "command": "synapseed",
      "args": ["serve"],
      "env": {
        "RUST_LOG": "info"
      }
    }
  }
}
```

:::

## Project-Specific Configuration

To analyze a specific project, pass `--project`:

```json
{
  "mcpServers": {
    "synapseed": {
      "command": "synapseed",
      "args": ["serve", "--project", "/path/to/your/project"],
      "env": {
        "RUST_LOG": "info"
      }
    }
  }
}
```

## Verification

1. Restart Claude Desktop
2. Look for the MCP icon in the chat interface
3. Ask Claude: *"What tools do you have from synapseed?"*
4. Claude should list all 19 tools

## Self-Telemetry

To enable performance self-observation:

```json
{
  "mcpServers": {
    "synapseed": {
      "command": "synapseed",
      "args": ["serve", "--project", "/path/to/project"],
      "env": {
        "RUST_LOG": "info",
        "SYNAPSEED_SELF_TELEMETRY": "1"
      }
    }
  }
}
```

## Troubleshooting

- **Tools not appearing:** Check that `synapseed` is in your `PATH`. Run `synapseed --version` to verify.
- **Logs:** Set `RUST_LOG=debug` for verbose output. Logs go to stderr.
- **Permissions:** Ensure the binary has execute permissions (`chmod +x`).
