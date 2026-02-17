# SYNAPSEED VS Code Extension

Real-time metrics, diagnostics, architecture health, and project insights from **SYNAPSEED** — directly in VS Code.

## Features

### Sidebar Panel

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

- **SYNAPSEED: Refresh All** — Reload all panels
- **SYNAPSEED: Ask a Question** — Natural-language query via the Whisper intent router
- **SYNAPSEED: Open Dashboard** — Full HTML dashboard in a webview panel
- **SYNAPSEED: Run Janitor Scan** — Trigger a maintenance scan

### Auto-Refresh

- **On file save**: Diagnostics refresh automatically
- **Timer**: Configurable interval (default: 30s) for metrics and diagnostics
- **Status bar**: Shows build status (✓ clean / ✗ errors) with click-to-refresh

## Prerequisites

- [SYNAPSEED](https://github.com/fabriziosalmi/synapseed) installed and available in PATH
- A workspace with a `Cargo.toml` or `.synapseed/dna.yaml`

## Installation

### From Source

```bash
cd vscode-extension
npm install
npm run compile
```

Then press `F5` in VS Code to launch the Extension Development Host.

### Package as VSIX

```bash
npm run package
# Install: code --install-extension synapseed-0.1.0.vsix
```

## Configuration

| Setting | Default | Description |
| :--- | :--- | :--- |
| `synapseed.binaryPath` | `synapseed` | Path to the synapseed binary |
| `synapseed.autoRefreshInterval` | `30` | Auto-refresh interval in seconds (0 to disable) |
| `synapseed.refreshOnSave` | `true` | Refresh diagnostics on file save |
| `synapseed.showNotifications` | `true` | Show notifications for important events |

## Development

```bash
npm install          # Install dependencies
npm run watch        # Watch mode (recompiles on change)
# Press F5 to launch Extension Development Host
```

## Architecture

```
vscode-extension/
├── package.json          # Extension manifest (views, commands, config)
├── tsconfig.json         # TypeScript config
├── media/
│   └── synapseed-icon.svg  # Activity Bar icon
└── src/
    ├── extension.ts      # Entry point — activation, commands, auto-refresh
    ├── cli.ts            # CLI runner — executes synapseed commands
    ├── items.ts          # TreeItem helpers (kvItem, sectionItem, etc.)
    └── providers/
        ├── statusProvider.ts       # Project Status view
        ├── metricsProvider.ts      # Metrics view
        ├── diagnosticsProvider.ts  # Compiler Diagnostics view
        ├── architectureProvider.ts # Architecture Health view
        ├── gitProvider.ts          # Git History view
        ├── securityProvider.ts     # Security view
        ├── consistencyProvider.ts  # Consistency view
        └── janitorProvider.ts      # Janitor Proposals view
```

The extension calls `synapseed` CLI commands as subprocesses with `RUST_LOG=off` and parses their text or JSON output. No MCP server connection is needed — it works with the standalone binary.
