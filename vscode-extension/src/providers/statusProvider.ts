import * as vscode from 'vscode';
import { runSynapseed, parseTextOutput } from '../cli';
import { SynapseedItem, kvItem, sectionItem, errorItem, emptyItem, statusItem, loadingItem } from '../items';

export class StatusProvider implements vscode.TreeDataProvider<SynapseedItem> {
    private _onDidChange = new vscode.EventEmitter<SynapseedItem | undefined>();
    readonly onDidChangeTreeData = this._onDidChange.event;
    private items: SynapseedItem[] = [emptyItem('Click refresh to load')];

    refresh(): void { this.loadData(); }

    private async loadData(): Promise<void> {
        this.items = [loadingItem()];
        this._onDidChange.fire(undefined);

        try {
            const result = await runSynapseed(['status'], { cache: true, cacheTtlMs: 10_000 });
            if (!result.success && !result.stdout) {
                this.items = [errorItem(result.stderr || 'Failed')];
                this._onDidChange.fire(undefined);
                return;
            }

            const sections = parseTextOutput(result.stdout);
            const items: SynapseedItem[] = [];
            const main = sections.get('main');

            if (main) {
                const project = main.get('Project');
                const state = main.get('State');
                if (project) { items.push(kvItem('Project', project, 'folder-opened')); }
                if (state) {
                    const label = state.includes('Healthy') ? 'Healthy' : state.includes('Partial') ? 'Partial' : 'Virgin';
                    const icon = label === 'Healthy' ? 'pass-filled' : label === 'Partial' ? 'warning' : 'info';
                    items.push(kvItem('State', label, icon));

                    const build = state.match(/build_system:\s*(\w+)/);
                    if (build) { items.push(kvItem('Build System', build[1], 'tools')); }
                    const files = state.match(/file_count:\s*(\d+)/);
                    if (files) { items.push(kvItem('Source Files', files[1], 'files')); }
                }
            }

            // Plugins
            const plugins = sections.get('Plugins');
            if (plugins) {
                const pluginItems: SynapseedItem[] = [];
                for (const [key, val] of plugins) {
                    if (!key.startsWith('_item_')) { continue; }
                    const m = val.match(/\[(\w+)\]\s+(\w+)\s*\(priority:\s*(\d+)\)/);
                    if (m) {
                        const ok = m[1] === 'OK';
                        const tt = new vscode.MarkdownString(`**${m[2]}** — Priority: ${m[3]}\n\nStatus: ${ok ? '✅ Active' : '❌ Failed'}`);
                        pluginItems.push(new SynapseedItem(m[2], {
                            description: `priority ${m[3]}`,
                            icon: ok ? 'pass-filled' : 'error',
                            tooltip: tt,
                            contextValue: 'synapseed.plugin',
                        }));
                    }
                }
                items.push(sectionItem('Plugins', pluginItems, 'extensions'));
            }

            // Version / Performance
            items.push(kvItem('CLI Response', `${result.durationMs}ms`, 'clock'));

            this.items = items.length > 0 ? items : [emptyItem('No data')];
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
