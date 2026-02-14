import * as vscode from 'vscode';
import { runSynapseed, getProjectRoot } from './cli';
import { globalCache } from './cache';
import { DiagnosticBridge } from './diagnosticBridge';
import { SynapseedCodeLensProvider } from './codelens';
import { SynapseedFileDecorator } from './fileDecorator';
import { AskPanel } from './askPanel';
import { DashboardPanel } from './dashboard';
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
let statusBarGrade: vscode.StatusBarItem;
let statusBarDiag: vscode.StatusBarItem;
let statusBarSecurity: vscode.StatusBarItem;

export function activate(context: vscode.ExtensionContext) {
    console.log('SYNAPSEED extension activating...');

    // ── Providers ────────────────────────────────────────────────────
    const statusProvider = new StatusProvider();
    const metricsProvider = new MetricsProvider();
    const diagnosticsProvider = new DiagnosticsProvider();
    const architectureProvider = new ArchitectureProvider();
    const gitProvider = new GitProvider();
    const securityProvider = new SecurityProvider();
    const consistencyProvider = new ConsistencyProvider();
    const janitorProvider = new JanitorProvider();
    const telemetryProvider = new TelemetryProvider();

    // ── Diagnostic Bridge (native Problems panel) ────────────────────
    const diagBridge = new DiagnosticBridge();
    context.subscriptions.push(diagBridge);

    // ── CodeLens ─────────────────────────────────────────────────────
    const codeLensProvider = new SynapseedCodeLensProvider();
    context.subscriptions.push(
        vscode.languages.registerCodeLensProvider(
            [{ language: 'rust' }, { language: 'python' }, { language: 'typescript' }],
            codeLensProvider,
        ),
    );

    // ── File Decorations ─────────────────────────────────────────────
    const fileDecorator = new SynapseedFileDecorator();
    context.subscriptions.push(
        vscode.window.registerFileDecorationProvider(fileDecorator),
    );

    // ── Tree Views ───────────────────────────────────────────────────
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

    // ── Commands ─────────────────────────────────────────────────────
    const commands: [string, (...args: any[]) => any][] = [
        // Refresh commands
        ['synapseed.refresh', () => refreshAll()],
        ['synapseed.refreshStatus', () => { statusProvider.refresh(); metricsProvider.refresh(); }],
        ['synapseed.refreshDiagnostics', () => { diagnosticsProvider.refresh(); diagBridge.refresh(); }],
        ['synapseed.refreshArchitecture', () => architectureProvider.refresh()],
        ['synapseed.refreshGit', () => gitProvider.refresh()],
        ['synapseed.refreshSecurity', () => securityProvider.refresh()],
        ['synapseed.refreshConsistency', () => consistencyProvider.refresh()],
        ['synapseed.runJanitor', () => janitorProvider.refresh()],
        ['synapseed.refreshTelemetry', () => telemetryProvider.refresh()],

        // Reset
        ['synapseed.resetTelemetry', async () => {
            const result = await runSynapseed(['mcp', 'call', 'reset-telemetry', '{}']);
            if (result.stdout) { vscode.window.showInformationMessage(`SYNAPSEED: ${result.stdout}`); }
            telemetryProvider.refresh();
        }],

        // Dashboard
        ['synapseed.openDashboard', () => DashboardPanel.show(context.extensionUri)],

        // Ask — from command palette
        ['synapseed.askQuestion', async () => {
            const query = await vscode.window.showInputBox({
                prompt: 'Ask SYNAPSEED about your codebase',
                placeHolder: 'e.g., why is the login broken?',
            });
            if (query) { AskPanel.show(context.extensionUri, query); }
        }],

        // Ask — context menu on symbol
        ['synapseed.askAboutSymbol', async (symbolName: string, file: string, line: number) => {
            const query = `explain ${symbolName} in ${file} around line ${line}`;
            AskPanel.show(context.extensionUri, query);
        }],

        // Analyze file (for codelens)
        ['synapseed.analyzeFile', async (relPath: string) => {
            const query = `analyze the history and churn of ${relPath}`;
            AskPanel.show(context.extensionUri, query);
        }],

        // Lookup symbol (Ctrl+Shift+L)
        ['synapseed.lookupSymbol', async () => {
            const name = await vscode.window.showInputBox({
                prompt: 'Symbol name to look up',
                placeHolder: 'e.g., SemanticIndex',
            });
            if (!name) { return; }

            const result = await vscode.window.withProgress(
                { location: vscode.ProgressLocation.Notification, title: `Looking up ${name}...` },
                () => runSynapseed(['lookup', name]),
            );
            if (result.stdout) {
                const output = vscode.window.createOutputChannel('SYNAPSEED Lookup');
                output.clear();
                output.appendLine(`Symbol: ${name}\n`);
                output.appendLine(result.stdout);
                output.show();
            }
        }],

        // Search (Ctrl+Shift+S)
        ['synapseed.searchCode', async () => {
            const query = await vscode.window.showInputBox({
                prompt: 'Search for code by concept',
                placeHolder: 'e.g., authentication login',
            });
            if (!query) { return; }

            const result = await vscode.window.withProgress(
                { location: vscode.ProgressLocation.Notification, title: `Searching: ${query}...` },
                () => runSynapseed(['search', query]),
            );
            if (result.stdout) {
                const output = vscode.window.createOutputChannel('SYNAPSEED Search');
                output.clear();
                output.appendLine(`Query: ${query}\n`);
                output.appendLine(result.stdout);
                output.show();
            }
        }],

        // Scan selection for secrets
        ['synapseed.scanSelection', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) { return; }
            const selection = editor.document.getText(editor.selection);
            if (!selection) {
                vscode.window.showWarningMessage('Select text to scan');
                return;
            }

            const result = await runSynapseed(['scan', '--content', selection]);
            const isClean = result.stdout?.includes('CLEAN');
            if (isClean) {
                vscode.window.showInformationMessage('$(pass) SYNAPSEED: Selection is clean — no secrets detected');
            } else {
                vscode.window.showWarningMessage(`$(shield) SYNAPSEED DLP Alert: ${result.stdout?.substring(0, 200)}`);
            }
        }],

        // Check command
        ['synapseed.checkCommand', async () => {
            const cmd = await vscode.window.showInputBox({
                prompt: 'Shell command to evaluate',
                placeHolder: 'e.g., rm -rf /tmp/build',
            });
            if (!cmd) { return; }

            const result = await runSynapseed(['check', cmd]);
            const allowed = result.stdout?.includes('ALLOWED');
            if (allowed) {
                vscode.window.showInformationMessage(`$(pass) Command ALLOWED: ${cmd}`);
            } else {
                vscode.window.showWarningMessage(`$(shield) Command DENIED: ${cmd}\n${result.stdout ?? ''}`);
            }
        }],

        // Quick blame (Ctrl+Shift+B)
        ['synapseed.blameCurrentFile', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) { return; }
            const root = getProjectRoot() ?? '';
            const relPath = editor.document.uri.fsPath.replace(root + '/', '');
            const line = editor.selection.active.line + 1;
            const startLine = Math.max(1, line - 5);
            const endLine = line + 5;

            const result = await vscode.window.withProgress(
                { location: vscode.ProgressLocation.Notification, title: `Git blame: ${relPath}:${line}` },
                () => runSynapseed(['blame', relPath, '-s', String(startLine), '-e', String(endLine)]),
            );

            if (result.stdout) {
                const output = vscode.window.createOutputChannel('SYNAPSEED Blame');
                output.clear();
                output.appendLine(`File: ${relPath}  Lines: ${startLine}-${endLine}\n`);
                output.appendLine(result.stdout);
                output.show(true);
            }
        }],

        // Invalidate cache
        ['synapseed.clearCache', () => {
            globalCache.invalidate();
            vscode.window.showInformationMessage('SYNAPSEED: Cache cleared');
            refreshAll();
        }],

        // Init project
        ['synapseed.initProject', async () => {
            const result = await vscode.window.withProgress(
                { location: vscode.ProgressLocation.Notification, title: 'Initializing SYNAPSEED...' },
                () => runSynapseed(['init'], { timeoutMs: 60_000 }),
            );
            if (result.success) {
                vscode.window.showInformationMessage('SYNAPSEED initialized! Refreshing...');
                refreshAll();
            } else {
                vscode.window.showErrorMessage(`Init failed: ${result.stderr}`);
            }
        }],
    ];

    for (const [id, handler] of commands) {
        context.subscriptions.push(vscode.commands.registerCommand(id, handler));
    }

    // ── Status Bar (3 segments) ──────────────────────────────────────
    statusBarGrade = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 52);
    statusBarGrade.command = 'synapseed.openDashboard';
    statusBarGrade.tooltip = 'Architecture Grade — Click to open dashboard';
    context.subscriptions.push(statusBarGrade);

    statusBarDiag = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 51);
    statusBarDiag.command = 'synapseed.refreshDiagnostics';
    statusBarDiag.tooltip = 'Build Status — Click to refresh diagnostics';
    context.subscriptions.push(statusBarDiag);

    statusBarSecurity = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 50);
    statusBarSecurity.command = 'synapseed.refreshSecurity';
    statusBarSecurity.tooltip = 'Security Status — Click to refresh';
    context.subscriptions.push(statusBarSecurity);

    statusBarGrade.show();
    statusBarDiag.show();
    statusBarSecurity.show();

    // ── File Save → Refresh ──────────────────────────────────────────
    context.subscriptions.push(
        vscode.workspace.onDidSaveTextDocument(() => {
            if (vscode.workspace.getConfiguration('synapseed').get<boolean>('refreshOnSave', true)) {
                globalCache.invalidate();
                diagnosticsProvider.refresh();
                diagBridge.refresh();
                updateStatusBar();
            }
        }),
    );

    // ── Auto-Refresh Timer ───────────────────────────────────────────
    setupAutoRefresh();
    context.subscriptions.push(
        vscode.workspace.onDidChangeConfiguration((e) => {
            if (e.affectsConfiguration('synapseed.autoRefreshInterval')) {
                setupAutoRefresh();
            }
        }),
    );

    // ── Initial Load ─────────────────────────────────────────────────
    refreshAll();
    console.log('SYNAPSEED extension activated.');

    // ── Inner Functions ──────────────────────────────────────────────
    function refreshAll() {
        globalCache.invalidate();
        statusProvider.refresh();
        metricsProvider.refresh();
        diagnosticsProvider.refresh();
        architectureProvider.refresh();
        gitProvider.refresh();
        securityProvider.refresh();
        consistencyProvider.refresh();
        telemetryProvider.refresh();
        diagBridge.refresh();
        codeLensProvider.refresh();
        fileDecorator.refresh();
        updateStatusBar();
    }

    function setupAutoRefresh() {
        if (autoRefreshTimer) { clearInterval(autoRefreshTimer); autoRefreshTimer = undefined; }
        const interval = vscode.workspace.getConfiguration('synapseed').get<number>('autoRefreshInterval', 30);
        if (interval > 0) {
            autoRefreshTimer = setInterval(() => {
                diagnosticsProvider.refresh();
                metricsProvider.refresh();
                telemetryProvider.refresh();
                diagBridge.refresh();
                updateStatusBar();
            }, interval * 1000);
        }
    }

    async function updateStatusBar() {
        try {
            // Grade
            const archResult = await runSynapseed(['architect'], { cache: true, cacheTtlMs: 60_000 });
            const gradeMatch = archResult.stdout?.match(/Grade:\s*(\w+)/);
            if (gradeMatch) {
                const g = gradeMatch[1];
                const emoji = g === 'A' ? '🟢' : g === 'B' ? '🔵' : g === 'C' ? '🟡' : '🔴';
                statusBarGrade.text = `${emoji} ${g}`;
            } else {
                statusBarGrade.text = '$(circuit-board) SYN';
            }

            // Diagnostics
            const diagResult = await runSynapseed(['diagnostics'], { cache: true, cacheTtlMs: 10_000 });
            if (diagResult.stdout?.startsWith('CLEAN')) {
                statusBarDiag.text = '$(pass) Build';
                statusBarDiag.backgroundColor = undefined;
            } else if (diagResult.stdout?.includes('error')) {
                statusBarDiag.text = '$(error) Build';
                statusBarDiag.backgroundColor = new vscode.ThemeColor('statusBarItem.errorBackground');
            } else {
                statusBarDiag.text = '$(circle-large-outline) Build';
                statusBarDiag.backgroundColor = undefined;
            }

            // Security
            statusBarSecurity.text = '$(shield) DLP';
        } catch {
            statusBarGrade.text = '$(warning) SYN';
        }
    }
}

export function deactivate() {
    if (autoRefreshTimer) { clearInterval(autoRefreshTimer); }
}
