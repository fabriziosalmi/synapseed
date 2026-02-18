# SYNAPSEED VS Code Extension

Real-time metrics, diagnostics, architecture health, and project insights from **SYNAPSEED** — directly in VS Code.

## Features

### Sidebar Panel

The SYNAPSEED icon in the Activity Bar opens a dedicated panel with **5 composite views**:

| View | Data Source | Description |
| :--- | :--- | :--- |
| **Overview** | `status`, `diagnose` | Project state, build system, file count, plugins, metrics, consistency score |
| **Diagnostics** | `diagnostics` | Live errors/warnings from the shadow compiler |
| **Code Quality** | `architect` | Grade (A–F), modules, coupling, violations, recommendations |
| **Security** | `scan`, `check`, `janitor`, `status` | DLP engine, sentinel stats, blocks, janitor proposals |
| **Git** | `diagnose`, `intent`, `blame` | Branch, HEAD, recent commits, intent summary |

### Commands (26)

| Category | Commands |
| :--- | :--- |
| **Refresh** | Refresh All, Refresh Overview, Refresh Diagnostics, Refresh Code Quality, Refresh Git, Refresh Security |
| **Panels** | Open Dashboard, Open Benchmark Results, Open Ask Panel, Focus Sidebar, Switch Panel… |
| **Ask / Search** | Ask a Question, Ask About Symbol, Ask About Active File, Lookup Symbol, Search Code by Concept |
| **Security** | Scan Selection for Secrets, Check Shell Command Safety |
| **Git** | Git Blame Current File, Analyze File History |
| **Session** | Clear Cache, Initialize Project, Export Conversation, Clear Conversation |
| **Layout** | Move Ask Panel: Beside Editor, Move Ask Panel: Center |

### Ask Panel

Interactive conversation panel with:
- Natural-language queries via the Whisper intent router
- Automatic file context detection from active editor
- Drag-and-drop files for analysis
- Conversation export and clear
- Layout control (beside editor / center)

### Dashboard

Full HTML dashboard in a webview panel with tabbed navigation for project overview, diagnostics, architecture, and security summaries.

### Benchmark Panel

View and compare reproducible benchmark results (SCR, F1, precision, recall) across runs.

### CodeLens

Inline CodeLens annotations on Rust, Python, and TypeScript files showing symbol metadata from the SYNAPSEED index.

### File Decorations

Risk badges in the Explorer tree for files with security findings or high churn scores.

### Keybindings

| Shortcut (Mac) | Command |
| :--- | :--- |
| `Cmd+Shift+A` | Ask a Question |
| `Cmd+Alt+L` | Lookup Symbol |
| `Cmd+Shift+F6` | Search Code by Concept |
| `Cmd+Alt+B` | Git Blame Current File |
| `Cmd+Alt+D` | Scan Selection for Secrets |
| `Cmd+Alt+R` | Refresh All |
| `Cmd+Shift+.` | Ask About Active File |

### Status Bar

Four status bar items showing:
- **Grade** — Overall project quality grade (A–F)
- **Diagnostics** — Error/warning counts with click-to-refresh
- **Security** — DLP block/denial counts
- **Session** — Health/distress detection from flight recorder

### Auto-Refresh

- **On file save**: Diagnostics refresh automatically (debounced 300ms)
- **Timer**: Configurable interval (default: 30s) for metrics and diagnostics
- **Workspace trust**: Extension activates only in trusted workspaces

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
code --install-extension synapseed-*.vsix
```

## Configuration

| Setting | Default | Description |
| :--- | :--- | :--- |
| `synapseed.binaryPath` | `synapseed` | Path to the synapseed binary |
| `synapseed.autoRefreshInterval` | `30` | Auto-refresh interval in seconds (0 to disable) |
| `synapseed.refreshOnSave` | `true` | Refresh diagnostics on file save |
| `synapseed.codeLens.enabled` | `true` | Show CodeLens annotations on source files |

## Development

```bash
npm install          # Install dependencies
npm run watch        # Watch mode (recompiles on change)
npm test             # Run unit tests (49 tests)
# Press F5 to launch Extension Development Host
```

## Architecture

```
vscode-extension/
├── package.json           # Extension manifest (views, commands, config)
├── tsconfig.json          # TypeScript config
├── media/
│   ├── synapseed-icon.svg # Activity Bar icon
│   └── walkthrough-*.md   # Getting Started walkthrough content
└── src/
    ├── extension.ts       # Entry point — activation, commands, auto-refresh
    ├── cli.ts             # CLI runner — executes synapseed commands
    ├── askPanel.ts        # Ask Panel webview (conversation UI)
    ├── dashboard.ts       # Dashboard webview (tabbed overview)
    ├── benchmarkPanel.ts  # Benchmark results webview
    ├── diagnosticBridge.ts # Maps CLI diagnostics to VS Code Problems panel
    ├── codelens.ts        # CodeLens provider for Rust/Python/TypeScript
    ├── fileDecorator.ts   # Explorer risk badges
    ├── dragDrop.ts        # Tree view drag-and-drop support
    ├── items.ts           # TreeItem helpers (kvItem, sectionItem, etc.)
    ├── constants.ts       # Shared constants (timeouts, cache TTL, etc.)
    ├── types.ts           # TypeScript interfaces
    ├── cache.ts           # LRU cache with TTL
    ├── html.ts            # HTML utilities (escaping, nonce, colors)
    ├── log.ts             # Output channel logger
    └── providers/
        ├── overviewProvider.ts      # Overview view
        ├── diagnosticsProvider.ts   # Diagnostics view
        ├── codeQualityProvider.ts   # Code Quality view
        ├── securityProvider.ts      # Security view
        └── gitProvider.ts           # Git view
```

The extension calls `synapseed` CLI commands as subprocesses with `RUST_LOG=off` and parses their text or JSON output. No MCP server connection is needed — it works with the standalone binary.
