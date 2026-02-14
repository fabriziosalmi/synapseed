import * as vscode from 'vscode';
import { runSynapseed, parseTextOutput } from '../cli';
import { SynapseedItem, kvItem, sectionItem, errorItem, emptyItem, loadingItem, progressItem } from '../items';

export class MetricsProvider implements vscode.TreeDataProvider<SynapseedItem> {
    private _onDidChange = new vscode.EventEmitter<SynapseedItem | undefined>();
    readonly onDidChangeTreeData = this._onDidChange.event;
    private items: SynapseedItem[] = [emptyItem('Click refresh to load')];

    refresh(): void { this.loadData(); }

    private async loadData(): Promise<void> {
        this.items = [loadingItem()];
        this._onDidChange.fire(undefined);

        try {
            const [statusRes, diagRes] = await Promise.all([
                runSynapseed(['status'], { cache: true, cacheTtlMs: 10_000 }),
                runSynapseed(['diagnose'], { cache: true, cacheTtlMs: 15_000 }),
            ]);

            const items: SynapseedItem[] = [];

            // Metrics from status
            if (statusRes.stdout) {
                const sec = parseTextOutput(statusRes.stdout);
                const metrics = sec.get('Metrics');
                if (metrics) {
                    for (const [k, v] of metrics) {
                        if (k.startsWith('_item_')) { continue; }
                        let icon = 'symbol-number';
                        if (k.toLowerCase().includes('file')) { icon = 'files'; }
                        else if (k.toLowerCase().includes('symbol')) { icon = 'symbol-method'; }
                        else if (k.toLowerCase().includes('dlp') || k.toLowerCase().includes('block')) { icon = 'shield'; }
                        else if (k.toLowerCase().includes('command')) { icon = 'terminal'; }
                        else if (k.toLowerCase().includes('error')) { icon = 'error'; }
                        else if (k.toLowerCase().includes('event')) { icon = 'zap'; }
                        items.push(kvItem(k, v, icon));
                    }
                }
            }

            // Consistency from diagnose
            if (diagRes.stdout) {
                const sec = parseTextOutput(diagRes.stdout);

                // Parse pipe-separated metrics line
                const diagMetrics = sec.get('Metrics');
                if (diagMetrics) {
                    for (const [k, v] of diagMetrics) {
                        if (!k.startsWith('_item_')) { continue; }
                        for (const part of v.split('|').map((p: string) => p.trim())) {
                            const kv = part.match(/^(\w[\w\s]*?):\s*(.+)$/);
                            if (kv && !items.find(i => i.label === kv[1].trim())) {
                                items.push(kvItem(kv[1].trim(), kv[2].trim(), 'symbol-number'));
                            }
                        }
                    }
                }

                const consistency = sec.get('Consistency');
                if (consistency) {
                    for (const [k, v] of consistency) {
                        if (!k.startsWith('_item_')) { continue; }
                        const m = v.match(/(\d+)\s+passed.*?(\d+)\s+failed.*?score:\s*(\d+)%/);
                        if (m) {
                            const pct = parseInt(m[3]);
                            items.push(progressItem('Consistency', pct, pct >= 90 ? 'pass-filled' : 'warning'));
                            items.push(kvItem('Checks Passed', m[1], 'check-all'));
                            const fails = parseInt(m[2]);
                            items.push(kvItem('Checks Failed', m[2], fails === 0 ? 'check-all' : 'error'));
                        }
                    }
                }
            }

            this.items = items.length > 0 ? items : [emptyItem('No metrics')];
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
