import * as vscode from 'vscode';
import { runSynapseed, parseTextOutput } from '../cli';
import { SynapseedItem, kvItem, errorItem, emptyItem } from '../items';

/**
 * Metrics view — files indexed, symbols, DLP stats, events, etc.
 */
export class MetricsProvider implements vscode.TreeDataProvider<SynapseedItem> {
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
            const result = await runSynapseed(['status']);
            if (!result.success && !result.stdout) {
                this.items = [errorItem(result.stderr || 'Failed to get metrics')];
                this._onDidChange.fire(undefined);
                return;
            }

            const sections = parseTextOutput(result.stdout);
            const items: SynapseedItem[] = [];

            // Metrics section
            const metrics = sections.get('Metrics');
            if (metrics) {
                for (const [key, val] of metrics) {
                    if (key.startsWith('_item_')) {
                        continue;
                    }
                    let icon = 'symbol-number';
                    if (key.toLowerCase().includes('file')) { icon = 'file'; }
                    else if (key.toLowerCase().includes('symbol')) { icon = 'symbol-method'; }
                    else if (key.toLowerCase().includes('dlp') || key.toLowerCase().includes('block')) { icon = 'shield'; }
                    else if (key.toLowerCase().includes('command')) { icon = 'terminal'; }
                    else if (key.toLowerCase().includes('error')) { icon = 'error'; }
                    else if (key.toLowerCase().includes('event')) { icon = 'zap'; }

                    items.push(kvItem(key, val, icon));
                }
            }

            // Also grab diagnose for extra data
            const diagResult = await runSynapseed(['diagnose']);
            if (diagResult.stdout) {
                const diagSections = parseTextOutput(diagResult.stdout);
                const diagMetrics = diagSections.get('Metrics');
                if (diagMetrics) {
                    // The diagnose metrics line is like: Files: 0 | Symbols: 0 | DLP Blocks: 0 | Events: 0
                    for (const [key, val] of diagMetrics) {
                        if (key.startsWith('_item_')) {
                            // Parse pipe-separated metrics
                            const parts = val.split('|').map((p: string) => p.trim());
                            for (const part of parts) {
                                const kv = part.match(/^(\w[\w\s]*?):\s*(.+)$/);
                                if (kv) {
                                    const existing = items.find(i => i.label === kv[1].trim());
                                    if (!existing) {
                                        items.push(kvItem(kv[1].trim(), kv[2].trim(), 'symbol-number'));
                                    }
                                }
                            }
                        }
                    }
                }

                const consistency = diagSections.get('Consistency');
                if (consistency) {
                    for (const [key, val] of consistency) {
                        if (key.startsWith('_item_')) {
                            // "54 passed, 0 failed (score: 100%)"
                            const m = val.match(/(\d+)\s+passed.*?(\d+)\s+failed.*?score:\s*(\d+)%/);
                            if (m) {
                                items.push(kvItem('Consistency Score', `${m[3]}%`, parseInt(m[3]) >= 90 ? 'pass' : 'warning'));
                                items.push(kvItem('Checks Passed', m[1], 'check'));
                                items.push(kvItem('Checks Failed', m[2], parseInt(m[2]) === 0 ? 'check' : 'error'));
                            } else {
                                items.push(kvItem('Consistency', val, 'info'));
                            }
                        }
                    }
                }
            }

            this.items = items.length > 0 ? items : [emptyItem('No metrics available')];
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
