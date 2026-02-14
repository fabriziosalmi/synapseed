import * as vscode from 'vscode';
import { runSynapseed } from './cli';
import { StatusProvider } from './providers/statusProvider';
import { MetricsProvider } from './providers/metricsProvider';
import { DiagnosticsProvider } from './providers/diagnosticsProvider';
import { ArchitectureProvider } from './providers/architectureProvider';
import { GitProvider } from './providers/gitProvider';
import { SecurityProvider } from './providers/securityProvider';
import { ConsistencyProvider } from './providers/consistencyProvider';
import { JanitorProvider } from './providers/janitorProvider';
import { TelemetryProvider } from './providers/telemetryProvider';

let autoRefreshTimer: NodeJS.Timeout | undefined;
let statusBar: vscode.StatusBarItem;

export function activate(context: vscode.ExtensionContext) {
    console.log('SYNAPSEED extension activating...');

    // ── Create providers ────────────────────────────────────────────
    const statusProvider = new StatusProvider();
    const metricsProvider = new MetricsProvider();
    const diagnosticsProvider = new DiagnosticsProvider();
    const architectureProvider = new ArchitectureProvider();
    const gitProvider = new GitProvider();
    const securityProvider = new SecurityProvider();
    const consistencyProvider = new ConsistencyProvider();
    const janitorProvider = new JanitorProvider();
    const telemetryProvider = new TelemetryProvider();

    // ── Register tree views ─────────────────────────────────────────
    context.subscriptions.push(
        vscode.window.registerTreeDataProvider('synapseed.status', statusProvider),
        vscode.window.registerTreeDataProvider('synapseed.metrics', metricsProvider),
        vscode.window.registerTreeDataProvider('synapseed.diagnostics', diagnosticsProvider),
        vscode.window.registerTreeDataProvider('synapseed.architecture', architectureProvider),
        vscode.window.registerTreeDataProvider('synapseed.git', gitProvider),
        vscode.window.registerTreeDataProvider('synapseed.security', securityProvider),
        vscode.window.registerTreeDataProvider('synapseed.consistency', consistencyProvider),
        vscode.window.registerTreeDataProvider('synapseed.janitor', janitorProvider),
        vscode.window.registerTreeDataProvider('synapseed.telemetry', telemetryProvider),
    );

    // ── Register commands ───────────────────────────────────────────
    context.subscriptions.push(
        vscode.commands.registerCommand('synapseed.refresh', () => {
            refreshAll();
        }),
        vscode.commands.registerCommand('synapseed.refreshStatus', () => {
            statusProvider.refresh();
            metricsProvider.refresh();
        }),
        vscode.commands.registerCommand('synapseed.refreshDiagnostics', () => {
            diagnosticsProvider.refresh();
        }),
        vscode.commands.registerCommand('synapseed.refreshArchitecture', () => {
            architectureProvider.refresh();
        }),
        vscode.commands.registerCommand('synapseed.refreshGit', () => {
            gitProvider.refresh();
        }),
        vscode.commands.registerCommand('synapseed.runJanitor', () => {
            janitorProvider.refresh();
        }),
        vscode.commands.registerCommand('synapseed.refreshTelemetry', () => {
            telemetryProvider.refresh();
        }),
        vscode.commands.registerCommand('synapseed.resetTelemetry', async () => {
            const result = await runSynapseed(['mcp', 'call', 'reset-telemetry', '{}']);
            if (result.stdout) {
                vscode.window.showInformationMessage(`SYNAPSEED: ${result.stdout}`);
            }
            telemetryProvider.refresh();
        }),
        vscode.commands.registerCommand('synapseed.openDashboard', async () => {
            const panel = vscode.window.createWebviewPanel(
                'synapseedDashboard',
                'SYNAPSEED Dashboard',
                vscode.ViewColumn.One,
                { enableScripts: true },
            );
            panel.webview.html = await buildDashboardHtml();
        }),
        vscode.commands.registerCommand('synapseed.askQuestion', async () => {
            const query = await vscode.window.showInputBox({
                prompt: 'Ask SYNAPSEED a question about your codebase',
                placeHolder: 'e.g., why is the login broken?',
            });
            if (query) {
                await askSynapseed(query);
            }
        }),
    );

    // ── Status bar item ─────────────────────────────────────────────
    statusBar = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 50);
    statusBar.text = '$(circuit-board) SYNAPSEED';
    statusBar.tooltip = 'Click to refresh all SYNAPSEED panels';
    statusBar.command = 'synapseed.refresh';
    statusBar.show();
    context.subscriptions.push(statusBar);

    // ── Auto-refresh on file save ───────────────────────────────────
    context.subscriptions.push(
        vscode.workspace.onDidSaveTextDocument((doc) => {
            const config = vscode.workspace.getConfiguration('synapseed');
            if (config.get<boolean>('refreshOnSave', true)) {
                // Only refresh diagnostics on save (lightweight)
                diagnosticsProvider.refresh();
                // Update status bar
                updateStatusBar();
            }
        }),
    );

    // ── Auto-refresh timer ──────────────────────────────────────────
    setupAutoRefresh();
    context.subscriptions.push(
        vscode.workspace.onDidChangeConfiguration((e) => {
            if (e.affectsConfiguration('synapseed.autoRefreshInterval')) {
                setupAutoRefresh();
            }
        }),
    );

    // ── Initial load ────────────────────────────────────────────────
    refreshAll();

    function refreshAll() {
        statusProvider.refresh();
        metricsProvider.refresh();
        diagnosticsProvider.refresh();
        architectureProvider.refresh();
        gitProvider.refresh();
        securityProvider.refresh();
        consistencyProvider.refresh();
        telemetryProvider.refresh();
        // Don't auto-refresh janitor — it's slow
        updateStatusBar();
    }

    function setupAutoRefresh() {
        if (autoRefreshTimer) {
            clearInterval(autoRefreshTimer);
            autoRefreshTimer = undefined;
        }
        const interval = vscode.workspace.getConfiguration('synapseed')
            .get<number>('autoRefreshInterval', 30);
        if (interval > 0) {
            autoRefreshTimer = setInterval(() => {
                // Lightweight refresh — just diagnostics and metrics
                diagnosticsProvider.refresh();
                metricsProvider.refresh();
                telemetryProvider.refresh();
                updateStatusBar();
            }, interval * 1000);
        }
    }

    async function updateStatusBar() {
        try {
            const result = await runSynapseed(['diagnostics']);
            if (result.stdout?.startsWith('CLEAN')) {
                statusBar.text = '$(pass) SYNAPSEED';
                statusBar.backgroundColor = undefined;
            } else if (result.stdout?.includes('error')) {
                statusBar.text = '$(error) SYNAPSEED';
                statusBar.backgroundColor = new vscode.ThemeColor('statusBarItem.errorBackground');
            } else {
                statusBar.text = '$(circuit-board) SYNAPSEED';
                statusBar.backgroundColor = undefined;
            }
        } catch {
            statusBar.text = '$(warning) SYNAPSEED';
        }
    }

    console.log('SYNAPSEED extension activated.');
}

async function askSynapseed(query: string): Promise<void> {
    const outputChannel = vscode.window.createOutputChannel('SYNAPSEED Ask');
    outputChannel.show();
    outputChannel.appendLine(`> ${query}\n`);
    outputChannel.appendLine('Thinking...\n');

    const result = await runSynapseed(['ask', query, '--raw']);
    outputChannel.clear();
    outputChannel.appendLine(`> ${query}\n`);
    if (result.stdout) {
        outputChannel.appendLine(result.stdout);
    } else {
        outputChannel.appendLine(`Error: ${result.stderr}`);
    }
}

async function buildDashboardHtml(): Promise<string> {
    // Gather all data
    const [statusResult, diagResult, archResult] = await Promise.all([
        runSynapseed(['status']),
        runSynapseed(['diagnostics']),
        runSynapseed(['architect']),
    ]);

    const archJson = archResult.stdout?.match(/(\{[\s\S]*\})/)?.[1] ?? '{}';

    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <style>
        body {
            font-family: var(--vscode-font-family, 'Segoe UI', sans-serif);
            color: var(--vscode-foreground);
            background: var(--vscode-editor-background);
            padding: 20px;
            line-height: 1.6;
        }
        h1 { color: var(--vscode-editor-foreground); border-bottom: 1px solid var(--vscode-panel-border); padding-bottom: 8px; }
        h2 { color: var(--vscode-editor-foreground); margin-top: 24px; }
        .card {
            border: 1px solid var(--vscode-panel-border);
            border-radius: 6px;
            padding: 16px;
            margin: 12px 0;
            background: var(--vscode-editor-background);
        }
        .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 12px; }
        .metric { text-align: center; }
        .metric .value { font-size: 2em; font-weight: bold; color: var(--vscode-charts-green); }
        .metric .label { font-size: 0.9em; opacity: 0.7; }
        .grade { font-size: 4em; font-weight: bold; }
        .grade.A { color: var(--vscode-charts-green); }
        .grade.B { color: var(--vscode-charts-blue); }
        .grade.C { color: var(--vscode-charts-yellow); }
        .grade.D, .grade.F { color: var(--vscode-charts-red); }
        pre {
            background: var(--vscode-textCodeBlock-background);
            padding: 12px;
            border-radius: 4px;
            overflow-x: auto;
            font-size: 0.85em;
        }
    </style>
</head>
<body>
    <h1>SYNAPSEED Dashboard</h1>

    <div class="grid">
        <div class="card metric">
            <div class="label">Architecture</div>
            <div class="grade ${archJson.includes('"grade"') ? JSON.parse(archJson).grade : 'A'}">${archJson.includes('"grade"') ? JSON.parse(archJson).grade : '?'}</div>
        </div>
        <div class="card metric">
            <div class="label">Diagnostics</div>
            <div class="value">${diagResult.stdout?.startsWith('CLEAN') ? '✓' : '!'}</div>
        </div>
    </div>

    <h2>Status</h2>
    <div class="card"><pre>${escapeHtml(statusResult.stdout || 'N/A')}</pre></div>

    <h2>Diagnostics</h2>
    <div class="card"><pre>${escapeHtml(diagResult.stdout || 'N/A')}</pre></div>

    <h2>Architecture</h2>
    <div class="card"><pre>${escapeHtml(archResult.stdout || 'N/A')}</pre></div>
</body>
</html>`;
}

function escapeHtml(text: string): string {
    return text
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;');
}

export function deactivate() {
    if (autoRefreshTimer) {
        clearInterval(autoRefreshTimer);
    }
}
