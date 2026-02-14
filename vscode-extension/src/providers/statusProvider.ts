import * as vscode from 'vscode';
import { runSynapseed, parseTextOutput } from '../cli';
import { SynapseedItem, kvItem, sectionItem, errorItem, emptyItem } from '../items';

/**
 * Project Status view — shows project state, build system, file count, plugins.
 */
export class StatusProvider implements vscode.TreeDataProvider<SynapseedItem> {
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
                this.items = [errorItem(result.stderr || 'Failed to get status')];
                this._onDidChange.fire(undefined);
                return;
            }

            const sections = parseTextOutput(result.stdout);
            const items: SynapseedItem[] = [];

            // Main section
            const main = sections.get('main');
            if (main) {
                const project = main.get('Project');
                const state = main.get('State');
                if (project) {
                    items.push(kvItem('Project', project, 'folder'));
                }
                if (state) {
                    // Parse state: HealthyWorkspace { build_system: Cargo, file_count: 17 }
                    const stateLabel = state.includes('Healthy') ? 'Healthy' :
                        state.includes('Partial') ? 'Partial' :
                            state.includes('Virgin') ? 'Virgin' : state;
                    const icon = stateLabel === 'Healthy' ? 'check' :
                        stateLabel === 'Partial' ? 'warning' : 'info';
                    items.push(kvItem('State', stateLabel, icon));

                    const buildMatch = state.match(/build_system:\s*(\w+)/);
                    if (buildMatch) {
                        items.push(kvItem('Build System', buildMatch[1], 'tools'));
                    }
                    const fileMatch = state.match(/file_count:\s*(\d+)/);
                    if (fileMatch) {
                        items.push(kvItem('Source Files', fileMatch[1], 'file'));
                    }
                }
            }

            // Plugins section
            const plugins = sections.get('Plugins');
            if (plugins) {
                const pluginItems: SynapseedItem[] = [];
                for (const [key, val] of plugins) {
                    if (key.startsWith('_item_')) {
                        // [OK] cortex (priority: 50)
                        const m = val.match(/\[(\w+)\]\s+(\w+)\s*\(priority:\s*(\d+)\)/);
                        if (m) {
                            const icon = m[1] === 'OK' ? 'pass' : 'error';
                            pluginItems.push(kvItem(m[2], `priority ${m[3]}`, icon));
                        } else {
                            pluginItems.push(kvItem(val, '', 'extensions'));
                        }
                    }
                }
                items.push(sectionItem('Plugins', pluginItems, 'extensions'));
            }

            this.items = items.length > 0 ? items : [emptyItem('No status data')];
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
