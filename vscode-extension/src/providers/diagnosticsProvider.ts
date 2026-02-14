import * as vscode from 'vscode';
import { runSynapseed, getProjectRoot } from '../cli';
import { SynapseedItem, kvItem, sectionItem, errorItem, emptyItem, loadingItem, fileItem } from '../items';

export class DiagnosticsProvider implements vscode.TreeDataProvider<SynapseedItem> {
    private _onDidChange = new vscode.EventEmitter<SynapseedItem | undefined>();
    readonly onDidChangeTreeData = this._onDidChange.event;
    private items: SynapseedItem[] = [emptyItem('Click refresh to load')];

    refresh(): void { this.loadData(); }

    private async loadData(): Promise<void> {
        this.items = [loadingItem()];
        this._onDidChange.fire(undefined);

        try {
            const result = await runSynapseed(['diagnostics']);
            if (!result.stdout) {
                this.items = [errorItem(result.stderr || 'No output')];
                this._onDidChange.fire(undefined);
                return;
            }

            const text = result.stdout;
            if (text.startsWith('CLEAN')) {
                const took = text.match(/took (\d+ms)/)?.[1] ?? '';
                this.items = [
                    new SynapseedItem('Build Clean', {
                        description: '0 errors, 0 warnings',
                        icon: 'pass-filled',
                        tooltip: new vscode.MarkdownString('✅ **No compiler issues detected**'),
                    }),
                    kvItem('Last Check', took, 'clock'),
                ];
                this._onDidChange.fire(undefined);
                return;
            }

            const root = getProjectRoot() ?? '';
            const errors: SynapseedItem[] = [];
            const warnings: SynapseedItem[] = [];

            for (const line of text.split('\n')) {
                const t = line.trim();
                if (!t) { continue; }

                const isError = t.toLowerCase().startsWith('error');
                const isWarn = t.toLowerCase().startsWith('warning');
                if (!isError && !isWarn) { continue; }

                const fm = t.match(/(?:-->|→|at)\s+([^\s:]+):(\d+)(?::(\d+))?/);
                const item = new SynapseedItem(t.substring(0, 120), {
                    icon: isError ? 'error' : 'warning',
                    tooltip: new vscode.MarkdownString(`\`\`\`\n${t}\n\`\`\``),
                    contextValue: 'synapseed.diagnostic',
                });

                if (fm) {
                    const fullPath = fm[1].startsWith('/') ? fm[1] : `${root}/${fm[1]}`;
                    const lineNum = parseInt(fm[2]) - 1;
                    const col = parseInt(fm[3] ?? '1') - 1;
                    item.command = {
                        command: 'vscode.open',
                        title: 'Go to Error',
                        arguments: [vscode.Uri.file(fullPath), { selection: new vscode.Range(lineNum, col, lineNum, col + 20) }],
                    };
                    item.description = `${fm[1]}:${fm[2]}`;
                }

                (isError ? errors : warnings).push(item);
            }

            const items: SynapseedItem[] = [];
            if (errors.length > 0) { items.push(sectionItem(`Errors`, errors, 'error')); }
            if (warnings.length > 0) { items.push(sectionItem(`Warnings`, warnings, 'warning', true)); }
            if (items.length === 0) { items.push(kvItem('Output', text.substring(0, 100), 'info')); }

            this.items = items;
        } catch (e: any) {
            this.items = [errorItem(e.message)];
        }
        this._onDidChange.fire(undefined);
    }

    getTreeItem(el: SynapseedItem): vscode.TreeItem { return el; }
    getChildren(el?: SynapseedItem): SynapseedItem[] {
        return el?.children ?? (el ? [] : this.items);
    }
}
