import * as vscode from 'vscode';
import { runSynapseed, parseTextOutput } from '../cli';
import { SynapseedItem, kvItem, sectionItem, errorItem, emptyItem } from '../items';

/**
 * Git History view — branch, recent commits, intent summary.
 */
export class GitProvider implements vscode.TreeDataProvider<SynapseedItem> {
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
            const items: SynapseedItem[] = [];

            // Get diagnose for git info
            const result = await runSynapseed(['diagnose']);
            if (result.stdout) {
                const sections = parseTextOutput(result.stdout);
                const git = sections.get('Git');
                if (git) {
                    const branch = git.get('Branch');
                    const head = git.get('HEAD');
                    const commits = git.get('Commits');
                    const dirty = git.get('Dirty');

                    if (branch) { items.push(kvItem('Branch', branch, 'git-branch')); }
                    if (head) { items.push(kvItem('HEAD', head, 'git-commit')); }
                    if (commits) { items.push(kvItem('Total Commits', commits, 'history')); }
                    if (dirty) {
                        const icon = dirty === 'false' ? 'pass' : 'warning';
                        items.push(kvItem('Dirty', dirty, icon));
                    }

                    // Recent commits
                    const recentItems: SynapseedItem[] = [];
                    for (const [key, val] of git) {
                        if (key.startsWith('_item_')) {
                            // "dedcc873 | Fabrizio Salmi | v4.14.0 — ..."
                            const parts = val.split('|').map((p: string) => p.trim());
                            if (parts.length >= 3) {
                                const hash = parts[0];
                                const message = parts.slice(2).join(' | ');
                                recentItems.push(kvItem(hash, message, 'git-commit'));
                            } else {
                                recentItems.push(kvItem(val, '', 'git-commit'));
                            }
                        }
                    }
                    if (recentItems.length > 0) {
                        items.push(sectionItem('Recent Commits', recentItems, 'history'));
                    }
                }
            }

            // Get intent summary
            const intentResult = await runSynapseed(['intent', '--limit', '10']);
            if (intentResult.stdout && !intentResult.stdout.includes('error')) {
                const intentLines = intentResult.stdout.split('\n').filter((l: string) => l.trim());
                const intentItems: SynapseedItem[] = [];
                for (const line of intentLines.slice(0, 15)) {
                    const trimmed = line.trim();
                    if (trimmed && !trimmed.startsWith('===') && !trimmed.startsWith('---')) {
                        // Category lines like "fix: 5 commits" or commit lines
                        const catMatch = trimmed.match(/^(\w+):\s+(\d+)\s+commit/i);
                        if (catMatch) {
                            const icon = catMatch[1] === 'fix' ? 'wrench' :
                                catMatch[1] === 'feature' ? 'add' :
                                    catMatch[1] === 'refactor' ? 'edit' :
                                        catMatch[1] === 'security' ? 'shield' : 'tag';
                            intentItems.push(kvItem(catMatch[1], `${catMatch[2]} commits`, icon));
                        }
                    }
                }
                if (intentItems.length > 0) {
                    items.push(sectionItem('Commit Intent', intentItems, 'tag'));
                }
            }

            this.items = items.length > 0 ? items : [emptyItem('No git data')];
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
