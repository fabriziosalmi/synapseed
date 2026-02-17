import * as vscode from 'vscode';
import { runSynapseed, runSynapseedJson, getProjectRoot, getProjectRootOrThrow } from './cli';
import { globalCache } from './cache';
import { log } from './log';
import { DiagnosticBridge } from './diagnosticBridge';
import { SynapseedCodeLensProvider } from './codelens';
import { SynapseedFileDecorator } from './fileDecorator';
import { AskPanel } from './askPanel';
import { DashboardPanel } from './dashboard';
import { BenchmarkPanel } from './benchmarkPanel';
import { createDragDropController } from './dragDrop';
import { OverviewProvider } from './providers/overviewProvider';
import { DiagnosticsProvider } from './providers/diagnosticsProvider';
import { CodeQualityProvider } from './providers/codeQualityProvider';
import { SecurityProvider } from './providers/securityProvider';
import { GitProvider } from './providers/gitProvider';
import { SynapseedItem } from './items';
import {
    STATUS_BAR_PRIORITY, CACHE_TTL, TIMEOUT, SAVE_DEBOUNCE_MS,
    BLAME_CONTEXT_LINES,
} from './constants';
import { ArchitectReport } from './types';

let autoRefreshTimer: NodeJS.Timeout | undefined;
let saveDebounceTimer: NodeJS.Timeout | undefined;
let statusBarGrade: vscode.StatusBarItem;
let statusBarDiag: vscode.StatusBarItem;
let statusBarSecurity: vscode.StatusBarItem;
let statusBarSession: vscode.StatusBarItem;

export function activate(context: vscode.ExtensionContext) {
    log.info('SYNAPSEED extension activating...');

    // ── Workspace trust check ─────────────────────────────────────────
    if (!vscode.workspace.isTrusted) {
        log.warn('Workspace is not trusted — SYNAPSEED features disabled');
        context.subscriptions.push(
            vscode.workspace.onDidGrantWorkspaceTrust(() => {
                log.info('Workspace trust granted — activating SYNAPSEED');
                activateCore(context);
            }),
        );
        return;
    }

    activateCore(context);
}

function activateCore(context: vscode.ExtensionContext) {
    // ── Providers (5 consolidated views) ─────────────────────────────
    const overviewProvider = new OverviewProvider();
    const diagnosticsProvider = new DiagnosticsProvider();
    const codeQualityProvider = new CodeQualityProvider();
    const securityProvider = new SecurityProvider();
    const gitProvider = new GitProvider();
    context.subscriptions.push(overviewProvider, diagnosticsProvider, codeQualityProvider, securityProvider, gitProvider);

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

    // ── Tree Views (with drag-and-drop support) ──────────────────────
    const dndController = createDragDropController();
    const treeViewDefs: [string, vscode.TreeDataProvider<SynapseedItem>][] = [
        ['synapseed.overview', overviewProvider],
        ['synapseed.diagnostics', diagnosticsProvider],
        ['synapseed.codeQuality', codeQualityProvider],
        ['synapseed.security', securityProvider],
        ['synapseed.git', gitProvider],
    ];
    for (const [id, provider] of treeViewDefs) {
        const tv = vscode.window.createTreeView(id, {
            treeDataProvider: provider,
            dragAndDropController: dndController,
            canSelectMany: true,
            showCollapseAll: true,
        });
        context.subscriptions.push(tv);
    }

    // ── Commands ─────────────────────────────────────────────────────
    const commands: [string, (...args: unknown[]) => void | Promise<void>][] = [
        // Refresh commands
        ['synapseed.refresh', () => refreshAll()],
        ['synapseed.refreshOverview', () => overviewProvider.refresh()],
        ['synapseed.refreshDiagnostics', () => { diagnosticsProvider.refresh(); diagBridge.refresh(); }],
        ['synapseed.refreshCodeQuality', () => codeQualityProvider.refresh()],
        ['synapseed.refreshGit', () => gitProvider.refresh()],
        ['synapseed.refreshSecurity', () => securityProvider.refresh()],

        // Dashboard
        ['synapseed.openDashboard', () => DashboardPanel.show(context.extensionUri)],

        // Benchmark Results
        ['synapseed.openBenchmark', () => BenchmarkPanel.show(context.extensionUri)],

        // Ask — from command palette
        ['synapseed.askQuestion', async () => {
            const query = await vscode.window.showInputBox({
                prompt: 'Ask SYNAPSEED about your codebase',
                placeHolder: 'e.g., why is the login broken?',
            });
            if (query) { AskPanel.show(context.extensionUri, query); }
        }],

        // Ask — show panel without query
        ['synapseed.openAskPanel', () => {
            AskPanel.show(context.extensionUri);
        }],

        // Ask — about active file
        ['synapseed.askAboutActiveFile', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) {
                vscode.window.showWarningMessage('No active file');
                return;
            }
            try {
                const root = getProjectRootOrThrow();
                const relPath = editor.document.uri.fsPath.replace(root + '/', '');
                AskPanel.show(context.extensionUri, `analyze and explain ${relPath}`);
            } catch (err) {
                log.warn('askAboutActiveFile failed', err);
            }
        }],

        // Export conversation
        ['synapseed.exportConversation', () => AskPanel.exportConversation()],

        // Clear conversation
        ['synapseed.clearConversation', () => AskPanel.clearConversation()],

        // Panel layout: move Ask panel to different columns
        ['synapseed.moveAskBeside', () => AskPanel.showInColumn(context.extensionUri, vscode.ViewColumn.Beside)],
        ['synapseed.moveAskCenter', () => AskPanel.showInColumn(context.extensionUri, vscode.ViewColumn.One)],

        // Focus sidebar
        ['synapseed.focusSidebar', () => vscode.commands.executeCommand('synapseed.overview.focus')],

        // Quick switch: cycle between open SYNAPSEED panels
        ['synapseed.cyclePanels', async () => {
            const items: vscode.QuickPickItem[] = [
                { label: '$(dashboard) Dashboard', description: 'Architecture overview', detail: 'synapseed.openDashboard' },
                { label: '$(graph) Benchmarks', description: 'Benchmark results viewer', detail: 'synapseed.openBenchmark' },
                { label: '$(comment-discussion) Ask Panel', description: 'Chat with SYNAPSEED', detail: 'synapseed.openAskPanel' },
                { label: '$(list-tree) Sidebar', description: 'Tree views', detail: 'synapseed.focusSidebar' },
                { label: '$(search) Search Code', description: 'Semantic search', detail: 'synapseed.searchCode' },
                { label: '$(git-commit) Git Blame', description: 'Current file blame', detail: 'synapseed.blameCurrentFile' },
            ];
            const pick = await vscode.window.showQuickPick(items, {
                placeHolder: 'Switch to SYNAPSEED panel...',
                matchOnDescription: true,
            });
            if (pick?.detail) {
                await vscode.commands.executeCommand(pick.detail);
            }
        }],

        // Ask — context menu on symbol
        ['synapseed.askAboutSymbol', async (symbolName: unknown, file: unknown, line: unknown) => {
            const query = `explain ${String(symbolName)} in ${String(file)} around line ${String(line)}`;
            AskPanel.show(context.extensionUri, query);
        }],

        // Analyze file (for codelens)
        ['synapseed.analyzeFile', async (relPath: unknown) => {
            const query = `analyze the history and churn of ${String(relPath)}`;
            AskPanel.show(context.extensionUri, query);
        }],

        // Lookup symbol
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

        // Search
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

        // Quick blame
        ['synapseed.blameCurrentFile', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) { return; }
            try {
                const root = getProjectRootOrThrow();
                const relPath = editor.document.uri.fsPath.replace(root + '/', '');
                const line = editor.selection.active.line + 1;
                const startLine = Math.max(1, line - BLAME_CONTEXT_LINES);
                const endLine = line + BLAME_CONTEXT_LINES;

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
            } catch (err) {
                log.warn('blameCurrentFile failed', err);
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
                () => runSynapseed(['init'], { timeoutMs: TIMEOUT.LONG }),
            );
            if (result.success) {
                vscode.window.showInformationMessage('SYNAPSEED initialized! Refreshing...');
                refreshAll();
            } else {
                vscode.window.showErrorMessage(`Init failed: ${result.stderr || result.stdout || 'Unknown error'}`);
            }
        }],
    ];

    for (const [id, handler] of commands) {
        context.subscriptions.push(vscode.commands.registerCommand(id, handler));
    }

    // ── Status Bar (4 segments) ──────────────────────────────────────
    statusBarGrade = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, STATUS_BAR_PRIORITY.GRADE);
    statusBarGrade.command = 'synapseed.openDashboard';
    statusBarGrade.tooltip = 'Architecture Grade — Click to open dashboard';
    context.subscriptions.push(statusBarGrade);

    statusBarDiag = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, STATUS_BAR_PRIORITY.DIAGNOSTICS);
    statusBarDiag.command = 'synapseed.refreshDiagnostics';
    statusBarDiag.tooltip = 'Build Status — Click to refresh diagnostics';
    context.subscriptions.push(statusBarDiag);

    statusBarSecurity = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, STATUS_BAR_PRIORITY.SECURITY);
    statusBarSecurity.command = 'synapseed.refreshSecurity';
    statusBarSecurity.tooltip = 'Security Status — Click to refresh';
    context.subscriptions.push(statusBarSecurity);

    statusBarSession = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, STATUS_BAR_PRIORITY.SESSION);
    statusBarSession.command = 'synapseed.openDashboard';
    statusBarSession.tooltip = 'Session Health — Click for details';
    context.subscriptions.push(statusBarSession);

    statusBarGrade.show();
    statusBarDiag.show();
    statusBarSecurity.show();
    statusBarSession.show();

    // ── File Save → Debounced Refresh ────────────────────────────────
    context.subscriptions.push(
        vscode.workspace.onDidSaveTextDocument(() => {
            if (vscode.workspace.getConfiguration('synapseed').get<boolean>('refreshOnSave', true)) {
                if (saveDebounceTimer) { clearTimeout(saveDebounceTimer); }
                saveDebounceTimer = setTimeout(() => {
                    globalCache.invalidate();
                    diagnosticsProvider.refresh();
                    diagBridge.refresh();
                    updateStatusBar();
                }, SAVE_DEBOUNCE_MS);
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
    log.info('SYNAPSEED extension activated');

    // ── Inner Functions ──────────────────────────────────────────────
    function refreshAll() {
        globalCache.invalidate();
        overviewProvider.refresh();
        diagnosticsProvider.refresh();
        codeQualityProvider.refresh();
        securityProvider.refresh();
        gitProvider.refresh();
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
                overviewProvider.refresh();
                diagBridge.refresh();
                updateStatusBar();
            }, interval * 1000);
        }
    }

    async function updateStatusBar() {
        try {
            // Grade — use typed JSON response
            const archReport = await runSynapseedJson<ArchitectReport>(['architect'], { cache: true, cacheTtlMs: CACHE_TTL.ARCHITECTURE });
            if (archReport?.grade) {
                statusBarGrade.text = `$(circuit-board) ${archReport.grade}`;
            } else {
                statusBarGrade.text = '$(circuit-board) SYN';
            }

            // Diagnostics
            const diagResult = await runSynapseed(['diagnostics'], { cache: true, cacheTtlMs: CACHE_TTL.DIAGNOSTICS });
            const diagOutput = diagResult.stdout?.toLowerCase() ?? '';
            if (diagResult.stdout?.startsWith('CLEAN')) {
                statusBarDiag.text = '$(pass) Build';
                statusBarDiag.backgroundColor = undefined;
            } else if (diagOutput.includes('error')) {
                statusBarDiag.text = '$(error) Build';
                statusBarDiag.backgroundColor = new vscode.ThemeColor('statusBarItem.errorBackground');
            } else {
                statusBarDiag.text = '$(circle-large-outline) Build';
                statusBarDiag.backgroundColor = undefined;
            }

            // Security
            statusBarSecurity.text = '$(shield) DLP';

            // Session health — detect distress from diagnose output
            try {
                const sessionResult = await runSynapseed(['diagnose'], { cache: true, cacheTtlMs: CACHE_TTL.DEFAULT });
                const output = sessionResult.stdout ?? '';
                let moment = '';
                let loopAlert = false;
                try {
                    const data = JSON.parse(output);
                    moment = data?.flight_recorder?.cognitive_ledger?.moment ?? '';
                    loopAlert = !!data?.flight_recorder?.loop_alert;
                } catch {
                    // CLI text mode: check for keywords
                    loopAlert = output.includes('LOOP DETECTED') || output.includes('Iterative Distress');
                    moment = loopAlert ? 'Iterative Distress' : '';
                }

                if (loopAlert || moment === 'Iterative Distress') {
                    statusBarSession.text = '$(warning) Distress';
                    statusBarSession.backgroundColor = new vscode.ThemeColor('statusBarItem.warningBackground');
                    statusBarSession.tooltip = 'Session Health: Iterative Distress detected — the AI may be looping. Consider a different approach.';
                } else if (moment && moment !== 'Unknown') {
                    statusBarSession.text = `$(pulse) ${moment}`;
                    statusBarSession.backgroundColor = undefined;
                    statusBarSession.tooltip = `Session Phase: ${moment}`;
                } else {
                    statusBarSession.text = '$(pulse) Ready';
                    statusBarSession.backgroundColor = undefined;
                    statusBarSession.tooltip = 'Session Health: Normal';
                }
            } catch (err) {
                log.warn('Session health check failed', err);
                statusBarSession.text = '$(pulse) Ready';
                statusBarSession.backgroundColor = undefined;
            }
        } catch (err) {
            log.warn('Status bar update failed', err);
            statusBarGrade.text = '$(warning) SYN';
        }
    }
}

export function deactivate() {
    if (autoRefreshTimer) { clearInterval(autoRefreshTimer); }
    if (saveDebounceTimer) { clearTimeout(saveDebounceTimer); }
    log.dispose();
}
