import * as vscode from 'vscode';
import { runSynapseed } from '../cli';
import { SynapseedItem, kvItem, sectionItem, errorItem, emptyItem } from '../items';

/**
 * Janitor Proposals view — clippy warnings, unused deps, fix proposals.
 */
export class JanitorProvider implements vscode.TreeDataProvider<SynapseedItem> {
    private _onDidChange = new vscode.EventEmitter<SynapseedItem | undefined>();
    readonly onDidChangeTreeData = this._onDidChange.event;

    private items: SynapseedItem[] = [emptyItem('Run janitor scan to check for issues')];

    refresh(): void {
        this.loadData();
    }

    private async loadData(): Promise<void> {
        this.items = [new SynapseedItem('Scanning...', 'This may take a moment', vscode.TreeItemCollapsibleState.None, undefined, 'loading~spin')];
        this._onDidChange.fire(undefined);

        try {
            const result = await runSynapseed(['janitor'], 120000); // 2 min timeout
            if (!result.stdout) {
                this.items = [emptyItem(result.stderr || 'Janitor scan started in background')];
                this._onDidChange.fire(undefined);
                return;
            }

            const text = result.stdout;
            const items: SynapseedItem[] = [];

            // Parse janitor output
            if (text.includes('background') || text.includes('Background')) {
                items.push(kvItem('Status', 'Scan running in background', 'sync~spin'));
                items.push(new SynapseedItem(
                    'Check again in a few seconds',
                    undefined,
                    vscode.TreeItemCollapsibleState.None,
                    undefined,
                    'info'
                ));
            } else if (text.includes('No findings') || text.includes('clean')) {
                items.push(kvItem('Status', 'Clean — no issues found', 'pass'));
            } else {
                // Parse findings
                const lines = text.split('\n');
                const clippyItems: SynapseedItem[] = [];
                const depItems: SynapseedItem[] = [];
                const proposalItems: SynapseedItem[] = [];

                for (const line of lines) {
                    const trimmed = line.trim();
                    if (!trimmed || trimmed.startsWith('===') || trimmed.startsWith('---')) {
                        continue;
                    }

                    if (trimmed.includes('clippy') || trimmed.includes('warning[')) {
                        clippyItems.push(kvItem('Clippy', trimmed, 'warning'));
                    } else if (trimmed.includes('unused') && trimmed.includes('dep')) {
                        depItems.push(kvItem('Unused Dep', trimmed, 'trash'));
                    } else if (trimmed.includes('proposal') || trimmed.match(/^[0-9a-f-]{36}/)) {
                        proposalItems.push(kvItem('Proposal', trimmed, 'tools'));
                    }
                }

                if (clippyItems.length > 0) {
                    items.push(sectionItem('Clippy Warnings', clippyItems, 'warning'));
                }
                if (depItems.length > 0) {
                    items.push(sectionItem('Unused Dependencies', depItems, 'trash'));
                }
                if (proposalItems.length > 0) {
                    items.push(sectionItem('Fix Proposals', proposalItems, 'tools'));
                }

                if (items.length === 0) {
                    // Show raw output
                    items.push(kvItem('Output', text.substring(0, 200), 'info'));
                }
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
