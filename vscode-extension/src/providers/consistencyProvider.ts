import * as vscode from 'vscode';
import { runSynapseed, parseTextOutput } from '../cli';
import { SynapseedItem, kvItem, sectionItem, errorItem, emptyItem } from '../items';

/**
 * Consistency view — oracle check results.
 */
export class ConsistencyProvider implements vscode.TreeDataProvider<SynapseedItem> {
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
            const result = await runSynapseed(['diagnose']);
            if (!result.stdout) {
                this.items = [errorItem('No output')];
                this._onDidChange.fire(undefined);
                return;
            }

            const sections = parseTextOutput(result.stdout);
            const items: SynapseedItem[] = [];

            const consistency = sections.get('Consistency');
            if (consistency) {
                for (const [key, val] of consistency) {
                    if (key.startsWith('_item_')) {
                        // "54 passed, 0 failed (score: 100%)"
                        const m = val.match(/(\d+)\s+passed.*?(\d+)\s+failed.*?score:\s*(\d+)%/);
                        if (m) {
                            const score = parseInt(m[3]);
                            const scoreIcon = score >= 90 ? 'pass' : score >= 70 ? 'warning' : 'error';
                            items.push(kvItem('Score', `${m[3]}%`, scoreIcon));
                            items.push(kvItem('Passed', m[1], 'check'));
                            const failed = parseInt(m[2]);
                            items.push(kvItem('Failed', m[2], failed === 0 ? 'check' : 'error'));
                        } else {
                            items.push(kvItem('Result', val, 'info'));
                        }
                    }
                }
            }

            // Also run oracle to check for drift
            const oracleResult = await runSynapseed(['oracle']);
            if (oracleResult.stdout) {
                const text = oracleResult.stdout;
                if (text.includes('no drift') || text.includes('No drift') || text.includes('0 changes')) {
                    items.push(kvItem('Documentation Drift', 'None detected', 'pass'));
                } else {
                    // Parse changes
                    const changeLines = text.split('\n').filter((l: string) => l.trim() && !l.startsWith('==='));
                    const changeItems = changeLines.slice(0, 10).map((l: string) =>
                        kvItem('Drift', l.trim(), 'warning')
                    );
                    if (changeItems.length > 0) {
                        items.push(sectionItem('Documentation Drift', changeItems, 'warning'));
                    }
                }
            }

            this.items = items.length > 0 ? items : [emptyItem('No consistency data')];
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
