import * as vscode from 'vscode';
import { runSynapseed } from '../cli';
import { SynapseedItem, kvItem, sectionItem, errorItem, emptyItem } from '../items';

/**
 * Compiler Diagnostics view — errors/warnings from shadow compiler.
 */
export class DiagnosticsProvider implements vscode.TreeDataProvider<SynapseedItem> {
    private _onDidChange = new vscode.EventEmitter<SynapseedItem | undefined>();
    readonly onDidChangeTreeData = this._onDidChange.event;

    private items: SynapseedItem[] = [emptyItem('Click refresh to load')];

    refresh(): void {
        this.loadData();
    }

    private async loadData(): Promise<void> {
        this.items = [new SynapseedItem('Loading...', undefined, vscode.TreeItemCollapsibleState.None, undefined, 'loading~spin')];
        this._onDidChange.fire(undefined);

        try {
            const result = await runSynapseed(['diagnostics']);
            if (!result.stdout) {
                this.items = [errorItem(result.stderr || 'No output')];
                this._onDidChange.fire(undefined);
                return;
            }

            const text = result.stdout;

            // CLEAN: No diagnostics
            if (text.startsWith('CLEAN')) {
                this.items = [
                    new SynapseedItem('No Issues', 'Build is clean', vscode.TreeItemCollapsibleState.None, undefined, 'pass'),
                    kvItem('Last Check', text.match(/took (\d+ms)/)?.[1] ?? 'unknown', 'clock'),
                ];
                this._onDidChange.fire(undefined);
                return;
            }

            // Parse diagnostics output — typically JSON after text header
            const items: SynapseedItem[] = [];
            const lines = text.split('\n');
            const errors: SynapseedItem[] = [];
            const warnings: SynapseedItem[] = [];

            for (const line of lines) {
                const trimmed = line.trim();
                if (!trimmed) { continue; }

                // Try to detect error/warning lines
                // Format: ERROR: path:line:col — message
                // or: warning[code]: message -> path:line
                if (trimmed.toLowerCase().startsWith('error')) {
                    const item = new SynapseedItem(trimmed, undefined, vscode.TreeItemCollapsibleState.None, undefined, 'error');
                    // Try to extract file location for click-to-navigate
                    const fileMatch = trimmed.match(/(?:-->|→|at)\s+([^\s:]+):(\d+)/);
                    if (fileMatch) {
                        const root = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
                        if (root) {
                            const uri = vscode.Uri.file(`${root}/${fileMatch[1]}`);
                            item.command = {
                                command: 'vscode.open',
                                arguments: [uri, { selection: new vscode.Range(parseInt(fileMatch[2]) - 1, 0, parseInt(fileMatch[2]) - 1, 0) }],
                                title: 'Open File',
                            };
                        }
                    }
                    errors.push(item);
                } else if (trimmed.toLowerCase().startsWith('warning')) {
                    const item = new SynapseedItem(trimmed, undefined, vscode.TreeItemCollapsibleState.None, undefined, 'warning');
                    const fileMatch = trimmed.match(/(?:-->|→|at)\s+([^\s:]+):(\d+)/);
                    if (fileMatch) {
                        const root = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
                        if (root) {
                            const uri = vscode.Uri.file(`${root}/${fileMatch[1]}`);
                            item.command = {
                                command: 'vscode.open',
                                arguments: [uri, { selection: new vscode.Range(parseInt(fileMatch[2]) - 1, 0, parseInt(fileMatch[2]) - 1, 0) }],
                                title: 'Open File',
                            };
                        }
                    }
                    warnings.push(item);
                }
            }

            if (errors.length > 0) {
                items.push(sectionItem(`Errors`, errors, 'error'));
            }
            if (warnings.length > 0) {
                items.push(sectionItem(`Warnings`, warnings, 'warning'));
            }

            if (items.length === 0) {
                // Fallback: show raw output
                items.push(kvItem('Output', text.substring(0, 100), 'info'));
            }

            this.items = items;
        } catch (e: any) {
            this.items = [errorItem(e.message)];
        }

        this._onDidChange.fire(undefined);
    }

    getTreeItem(element: SynapseedItem): vscode.TreeItem {
        return element;
    }

    getChildren(element?: SynapseedItem): SynapseedItem[] {
        if (element?.children) {
            return element.children;
        }
        return element ? [] : this.items;
    }
}
