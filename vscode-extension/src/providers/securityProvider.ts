import * as vscode from 'vscode';
import { runSynapseed } from '../cli';
import { SynapseedItem, kvItem, sectionItem, errorItem, emptyItem, loadingItem, statusItem } from '../items';

export class SecurityProvider implements vscode.TreeDataProvider<SynapseedItem> {
    private _onDidChange = new vscode.EventEmitter<SynapseedItem | undefined>();
    readonly onDidChangeTreeData = this._onDidChange.event;
    private items: SynapseedItem[] = [emptyItem('Click refresh to load')];

    refresh(): void { this.loadData(); }

    private async loadData(): Promise<void> {
        this.items = [loadingItem()];
        this._onDidChange.fire(undefined);

        try {
            const items: SynapseedItem[] = [];

            const [scanRes, checkRes, statusRes] = await Promise.all([
                runSynapseed(['scan', '--content', 'test health check'], { cache: true, cacheTtlMs: 30_000 }),
                runSynapseed(['check', 'cargo test'], { cache: true, cacheTtlMs: 30_000 }),
                runSynapseed(['status'], { cache: true, cacheTtlMs: 10_000 }),
            ]);

            // Engine status
            if (scanRes.stdout) {
                const ok = scanRes.stdout.includes('CLEAN');
                items.push(statusItem('DLP Engine', ok, 'Active & Scanning', 'Alert'));
            }
            if (checkRes.stdout) {
                const ok = checkRes.stdout.includes('ALLOWED');
                items.push(statusItem('Command Sentinel', ok, 'Active & Guarding', 'Restricted'));
            }

            // Stats
            if (statusRes.stdout) {
                const patterns: Array<[string, string, string]> = [
                    ['DLP Scans', 'search', /DLP Scans:\s+(\d+)/.source],
                    ['DLP Blocks', 'shield', /DLP Blocks:\s+(\d+)/.source],
                    ['Commands Allowed', 'pass', /Commands Allowed:\s+(\d+)/.source],
                    ['Commands Denied', 'error', /Commands Denied:\s+(\d+)/.source],
                    ['Errors Prevented', 'shield', /Errors Prevented:\s+(\d+)/.source],
                ];
                for (const [name, icon, pat] of patterns) {
                    const m = statusRes.stdout.match(new RegExp(pat));
                    if (m) {
                        const isAlert = (name.includes('Block') || name.includes('Denied')) && parseInt(m[1]) > 0;
                        items.push(kvItem(name, m[1], isAlert ? 'warning' : icon));
                    }
                }
            }

            // Security summary tooltip
            const summary = new vscode.MarkdownString([
                '## Security Summary',
                '- **DLP Shield**: Aho-Corasick + regex pattern matching',
                '- **Command Sentinel**: Deny-first policy evaluation',
                '- **Network**: 127.0.0.1 only, zero outbound',
                '- **Process**: Read-only AST, no subprocess spawning',
            ].join('\n'));
            items.push(new SynapseedItem('Defense-in-Depth', {
                description: 'fail-closed model',
                icon: 'lock',
                tooltip: summary,
            }));

            this.items = items.length > 0 ? items : [emptyItem('No security data')];
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
