import * as vscode from 'vscode';
import { runSynapseed, parseTextOutput } from '../cli';
import { SynapseedItem, kvItem, sectionItem, errorItem, emptyItem, loadingItem, progressItem } from '../items';

export class ConsistencyProvider implements vscode.TreeDataProvider<SynapseedItem> {
    private _onDidChange = new vscode.EventEmitter<SynapseedItem | undefined>();
    readonly onDidChangeTreeData = this._onDidChange.event;
    private items: SynapseedItem[] = [emptyItem('Click refresh to load')];

    refresh(): void { this.loadData(); }

    private async loadData(): Promise<void> {
        this.items = [loadingItem()];
        this._onDidChange.fire(undefined);

        try {
            const [diagRes, oracleRes] = await Promise.all([
                runSynapseed(['diagnose'], { cache: true, cacheTtlMs: 15_000 }),
                runSynapseed(['oracle'], { timeoutMs: 60_000 }),
            ]);

            const items: SynapseedItem[] = [];

            if (diagRes.stdout) {
                const sec = parseTextOutput(diagRes.stdout);
                const consistency = sec.get('Consistency');
                if (consistency) {
                    for (const [k, v] of consistency) {
                        if (!k.startsWith('_item_')) { continue; }
                        const m = v.match(/(\d+)\s+passed.*?(\d+)\s+failed.*?score:\s*(\d+)%/);
                        if (m) {
                            const pct = parseInt(m[3]);
                            items.push(progressItem('Consistency Score', pct, pct >= 90 ? 'pass-filled' : 'warning'));
                            items.push(kvItem('Checks Passed', m[1], 'check-all'));
                            const fails = parseInt(m[2]);
                            items.push(kvItem('Checks Failed', m[2], fails === 0 ? 'check-all' : 'error'));
                        }
                    }
                }
            }

            if (oracleRes.stdout) {
                const text = oracleRes.stdout;
                if (text.includes('no drift') || text.includes('No drift') || text.includes('0 changes')) {
                    items.push(kvItem('Documentation Drift', 'None detected', 'pass-filled'));
                } else {
                    const changes = text.split('\n')
                        .filter((l: string) => l.trim() && !l.startsWith('==='))
                        .slice(0, 10)
                        .map((l: string) => new SynapseedItem(l.trim(), { icon: 'warning' }));
                    if (changes.length > 0) {
                        items.push(sectionItem('Documentation Drift', changes, 'warning'));
                    }
                }
            }

            this.items = items.length > 0 ? items : [emptyItem('No consistency data')];
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
