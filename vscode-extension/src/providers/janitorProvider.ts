import * as vscode from 'vscode';
import { runSynapseed } from '../cli';
import { SynapseedItem, kvItem, sectionItem, errorItem, emptyItem, loadingItem } from '../items';

export class JanitorProvider implements vscode.TreeDataProvider<SynapseedItem> {
    private _onDidChange = new vscode.EventEmitter<SynapseedItem | undefined>();
    readonly onDidChangeTreeData = this._onDidChange.event;
    private items: SynapseedItem[] = [emptyItem('Run janitor scan to check for issues')];

    refresh(): void { this.loadData(); }

    private async loadData(): Promise<void> {
        this.items = [new SynapseedItem('Scanning...', { description: 'This may take a moment', icon: 'loading~spin' })];
        this._onDidChange.fire(undefined);

        try {
            const result = await runSynapseed(['janitor'], { timeoutMs: 120_000 });
            if (!result.stdout) {
                this.items = [emptyItem(result.stderr || 'Scan started in background')];
                this._onDidChange.fire(undefined);
                return;
            }

            const text = result.stdout;
            const items: SynapseedItem[] = [];

            if (text.includes('background') || text.includes('Background')) {
                items.push(kvItem('Status', 'Scan running in background', 'sync~spin'));
                items.push(emptyItem('Refresh in a few seconds'));
            } else if (text.includes('No findings') || text.includes('clean')) {
                items.push(new SynapseedItem('All Clean', {
                    description: 'No issues found',
                    icon: 'pass-filled',
                    tooltip: new vscode.MarkdownString('✅ **No clippy warnings or unused dependencies**'),
                }));
            } else {
                const clippyItems: SynapseedItem[] = [];
                const depItems: SynapseedItem[] = [];
                const proposalItems: SynapseedItem[] = [];

                for (const line of text.split('\n')) {
                    const t = line.trim();
                    if (!t || t.startsWith('===') || t.startsWith('---')) { continue; }

                    if (t.includes('clippy') || t.includes('warning[')) {
                        clippyItems.push(new SynapseedItem(t.substring(0, 100), {
                            icon: 'warning',
                            tooltip: t,
                            contextValue: 'synapseed.clippy',
                        }));
                    } else if (t.includes('unused') && t.includes('dep')) {
                        depItems.push(new SynapseedItem(t.substring(0, 80), {
                            icon: 'trash',
                            tooltip: t,
                            contextValue: 'synapseed.unusedDep',
                        }));
                    } else if (t.includes('proposal') || t.match(/^[0-9a-f-]{36}/)) {
                        // Extract proposal ID for "apply fix" action
                        const idMatch = t.match(/([0-9a-f-]{36})/);
                        const item = new SynapseedItem(t.substring(0, 80), {
                            icon: 'tools',
                            tooltip: t,
                            contextValue: idMatch ? 'synapseed.proposal' : undefined,
                        });
                        proposalItems.push(item);
                    }
                }

                if (clippyItems.length > 0) { items.push(sectionItem('Clippy Warnings', clippyItems, 'warning')); }
                if (depItems.length > 0) { items.push(sectionItem('Unused Dependencies', depItems, 'trash')); }
                if (proposalItems.length > 0) { items.push(sectionItem('Fix Proposals', proposalItems, 'tools')); }

                if (items.length === 0) { items.push(kvItem('Output', text.substring(0, 200), 'info')); }
            }

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
