# VS Code Extension

Real-time metrics, diagnostics, architecture health, and project insights from **SYNAPSEED** — directly in VS Code.

::: info NEW IN v4.15.0
The SYNAPSEED VS Code Extension was first released in version 4.15.0, providing a comprehensive IDE integration for all SYNAPSEED features.
:::

## Overview

The extension brings SYNAPSEED's semantic middleware capabilities directly into your VS Code workspace with 9 dedicated sidebar panels, command palette integration, and automatic refresh capabilities.

## Features

### Sidebar Panels

The SYNAPSEED icon in the Activity Bar opens a dedicated panel with 9 views:

| View | Data Source | Description |
| :--- | :--- | :--- |
| **Project Status** | `synapseed status` | State, build system, file count, active plugins |
| **Metrics** | `synapseed status` + `diagnose` | Files indexed, symbols, DLP blocks, consistency score |
| **Compiler Diagnostics** | `synapseed diagnostics` | Live errors/warnings from the shadow compiler |
| **Architecture Health** | `synapseed architect` | Grade (A-F), modules, coupling, violations, recommendations |
| **Git History** | `synapseed diagnose` + `intent` | Branch, HEAD, recent commits, intent summary |
| **Security** | `synapseed scan` + `check` + `status` | DLP engine status, sentinel stats, blocks/denials |
| **Consistency** | `synapseed diagnose` + `oracle` | Oracle score, documentation drift detection |
| **Janitor Proposals** | `synapseed janitor` | Clippy warnings, unused deps, fix proposals |
| **Telemetry** | `synapseed status` + OTLP | Performance hotspots, span metrics, heatmap |

### Commands

Access these commands from the Command Palette (`Ctrl+Shift+P` / `Cmd+Shift+P`):

- **SYNAPSEED: Refresh All** — Reload all panels with fresh data
- **SYNAPSEED: Ask a Question** — Natural-language query via the Whisper intent router
- **SYNAPSEED: Open Dashboard** — Full HTML dashboard in a webview panel
- **SYNAPSEED: Run Janitor Scan** — Trigger a maintenance scan for clippy warnings and unused deps

### Auto-Refresh Capabilities

The extension keeps your data fresh automatically:

- **On file save**: Diagnostics refresh automatically when you save any file
- **Timer-based**: Configurable interval (default: 30 seconds) for metrics and diagnostics
- **Status bar**: Shows build status (✓ clean / ✗ errors) with click-to-refresh functionality

## Installation

### Prerequisites

1. **SYNAPSEED installed**: Ensure SYNAPSEED is installed and available in your PATH:
   ```bash
   cargo install --path bin/synapseed --force
   synapseed --version
   ```

2. **Project with Rust or SYNAPSEED config**: A workspace with either:
   - A `Cargo.toml` file
   - A `.synapseed/dna.yaml` configuration file

### Install from VSIX

1. **Build the extension**:
   ```bash
   cd vscode-extension
   npm install
   npm run package
   ```

2. **Install the `.vsix` file**:
   ```bash
   code --install-extension synapseed-0.3.0.vsix
   ```

3. **Reload VS Code**: Press `Ctrl+Shift+P` / `Cmd+Shift+P` and select "Developer: Reload Window"

### Development Mode

To develop or test the extension:

```bash
cd vscode-extension
npm install
npm run watch
```

Then press `F5` in VS Code to launch the Extension Development Host.

## Configuration

Configure the extension through VS Code settings (`Ctrl+,` / `Cmd+,`):

| Setting | Default | Description |
| :--- | :--- | :--- |
| `synapseed.binaryPath` | `synapseed` | Path to the synapseed binary. Set to absolute path if not in PATH |
| `synapseed.autoRefreshInterval` | `30` | Auto-refresh interval in seconds (0 to disable) |
| `synapseed.refreshOnSave` | `true` | Automatically refresh diagnostics on file save |
| `synapseed.showNotifications` | `true` | Show notifications for important events |

### Example Configuration

Add to your VS Code `settings.json`:

```json
{
  "synapseed.binaryPath": "/usr/local/bin/synapseed",
  "synapseed.autoRefreshInterval": 60,
  "synapseed.refreshOnSave": true,
  "synapseed.showNotifications": false
}
```

## Usage Guide

### Getting Started

1. **Open the SYNAPSEED panel**: Click the SYNAPSEED icon in the Activity Bar (left sidebar)
2. **Wait for initialization**: The extension will run initial scans and populate all views
3. **Explore the panels**: Click through each view to see different aspects of your project

### Common Workflows

#### Checking Project Health

1. Open the **Architecture Health** panel
2. Review the grade (A-F) and any violations
3. Click on specific recommendations for details
4. Use "SYNAPSEED: Consult Architect" for guidance

#### Reviewing Security

1. Open the **Security** panel
2. Check DLP engine status and blocks count
3. Review any recent security denials
4. Run manual scans with `synapseed scan`

#### Managing Code Quality

1. Open the **Janitor Proposals** panel
2. Review clippy warnings and unused dependencies
3. Click "Apply" to fix issues (preview in dry-run mode by default)
4. Use "SYNAPSEED: Run Janitor Scan" to refresh

#### Understanding Performance

1. Enable [self-telemetry](/integration/self-telemetry) in your project
2. Open the **Telemetry** panel
3. Review top performance hotspots
4. Correlate with source code locations

### Asking Questions

The "Ask a Question" command provides natural-language access to SYNAPSEED:

1. Press `Ctrl+Shift+P` / `Cmd+Shift+P`
2. Type "SYNAPSEED: Ask a Question"
3. Enter your question (e.g., "Why is login broken?", "What changed in auth?")
4. View the orchestrated response in a new editor tab

## Architecture

The extension is built with a clean provider pattern:

```
vscode-extension/
├── package.json          # Extension manifest (views, commands, config)
├── tsconfig.json         # TypeScript configuration
├── media/
│   ├── synapseed-icon.svg     # Activity Bar icon
│   └── walkthrough-*.md       # Getting Started walkthrough
└── src/
    ├── extension.ts           # Entry point — activation, commands, auto-refresh
    ├── cli.ts                 # CLI runner — executes synapseed commands
    ├── items.ts               # TreeItem helpers (kvItem, sectionItem)
    └── providers/
        ├── statusProvider.ts       # Project Status view
        ├── metricsProvider.ts      # Metrics view
        ├── diagnosticsProvider.ts  # Compiler Diagnostics view
        ├── architectureProvider.ts # Architecture Health view
        ├── gitProvider.ts          # Git History view
        ├── securityProvider.ts     # Security view
        ├── consistencyProvider.ts  # Consistency view
        ├── janitorProvider.ts      # Janitor Proposals view
        └── telemetryProvider.ts    # Telemetry view
```

### Design Principles

- **CLI-based**: Calls `synapseed` CLI commands as subprocesses (no MCP server connection required)
- **Async by default**: All operations are non-blocking
- **Error resilient**: Gracefully handles missing binaries or failed commands
- **Minimal dependencies**: Pure TypeScript with VS Code API

## Troubleshooting

### Extension Not Loading

**Problem**: SYNAPSEED icon doesn't appear in Activity Bar

**Solutions**:
1. Verify installation: `code --list-extensions | grep synapseed`
2. Check VS Code version: Requires VS Code 1.85.0 or later
3. Reload window: `Ctrl+Shift+P` → "Developer: Reload Window"

### No Data in Panels

**Problem**: Panels are empty or show errors

**Solutions**:
1. Verify SYNAPSEED is in PATH: `synapseed --version`
2. Check workspace has `Cargo.toml` or `.synapseed/dna.yaml`
3. Run "SYNAPSEED: Refresh All" command
4. Check Output panel for error messages

### Binary Not Found

**Problem**: "synapseed command not found" error

**Solutions**:
1. Install SYNAPSEED: `cargo install --path bin/synapseed --force`
2. Set absolute path in settings: `"synapseed.binaryPath": "/full/path/to/synapseed"`
3. Add SYNAPSEED to PATH in shell profile

### Slow Performance

**Problem**: Panels take too long to refresh

**Solutions**:
1. Increase auto-refresh interval: `"synapseed.autoRefreshInterval": 60`
2. Disable timer-based refresh: `"synapseed.autoRefreshInterval": 0`
3. Disable refresh-on-save: `"synapseed.refreshOnSave": false`
4. Check for large codebases: Consider using disk-based search index

## Comparison with MCP Integration

| Aspect | VS Code Extension | MCP Integration |
| :--- | :--- | :--- |
| **Connection** | CLI subprocess | stdio MCP server |
| **Use Case** | Interactive IDE panels | AI agent tooling |
| **Latency** | Higher (subprocess spawn) | Lower (persistent process) |
| **Setup** | Install extension | Configure MCP client |
| **Auto-refresh** | Built-in timer | On-demand via tool calls |
| **Best For** | Human developers | AI coding assistants |

Both approaches are complementary — use the extension for your own development workflow and MCP integration for AI assistance.

## Version History

### v0.3.0 (February 2026)
- **UI/UX Overhaul**: Tabbed dashboard, drag-and-drop panels, enriched Ask panel
- **Enhanced visualizations**: Interactive charts and graphs
- **Improved navigation**: Quick links to source code from any panel

### v0.1.0 (February 2026)
- Initial release with 9 sidebar panels
- Command palette integration
- Auto-refresh capabilities
- Status bar integration

## Further Reading

- [VS Code Extension Development](https://code.visualstudio.com/api)
- [SYNAPSEED CLI Reference](/reference/cli)
- [Self-Telemetry Setup](/integration/self-telemetry)
- [Architecture Overview](/architecture/overview)
