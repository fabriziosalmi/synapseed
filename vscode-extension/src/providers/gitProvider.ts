import * as vscode from 'vscode';
import { runSynapseed, parseTextOutput } from '../cli';
import { log } from '../log';
import { CACHE_TTL } from '../constants';
import { SynapseedItem, kvItem, sectionItem, errorItem, emptyItem, loadingItem } from '../items';

export class GitProvider implements vscode.TreeDataProvider<SynapseedItem>, vscode.Disposable {
    private _onDidChange = new vscode.EventEmitter<SynapseedItem | undefined>();
    readonly onDidChangeTreeData = this._onDidChange.event;
    private items: SynapseedItem[] = [emptyItem('Click refresh to load')];

    dispose(): void { this._onDidChange.dispose(); }
    refresh(): void { this.loadData(); }

    private async loadData(): Promise<void> {
        // Keep previous items visible during refresh (no flash)
        if (this.items.length <= 1) {
            this.items = [loadingItem()];
            this._onDidChange.fire(undefined);
        }

        try {
            const items: SynapseedItem[] = [];

            const [diagRes, intentRes] = await Promise.all([
                runSynapseed(['diagnose'], { cache: true, cacheTtlMs: CACHE_TTL.DEFAULT }),
                runSynapseed(['intent', '--limit', '10'], { cache: true, cacheTtlMs: CACHE_TTL.DEFAULT * 2 }),
            ]);

            if (diagRes.stdout) {
                const sec = parseTextOutput(diagRes.stdout);
                const git = sec.get('Git');
                if (git) {
                    const branch = git.get('Branch');
                    const head = git.get('HEAD');
                    const commits = git.get('Commits');
                    const dirty = git.get('Dirty');

                    if (branch) { items.push(kvItem('Branch', branch, 'git-branch')); }
                    if (head) {
                        const short = head.substring(0, 8);
                        items.push(kvItem('HEAD', short, 'git-commit', `Full: ${head}`));
                    }
                    if (commits) { items.push(kvItem('Total Commits', commits, 'history')); }
                    if (dirty) {
                        items.push(kvItem('Working Tree', dirty === 'false' ? 'Clean' : 'Dirty', dirty === 'false' ? 'pass-filled' : 'warning'));
                    }

                    // Recent commits
                    const commitItems: SynapseedItem[] = [];
                    for (const [key, val] of git) {
                        if (!key.startsWith('_item_')) { continue; }
                        const parts = val.split('|').map((p: string) => p.trim());
                        if (parts.length >= 3) {
                            const hash = parts[0].substring(0, 8);
                            const author = parts[1];
                            const message = parts.slice(2).join(' | ');
                            const tt = new vscode.MarkdownString(`**${hash}** by ${author}\n\n${message}`);
                            commitItems.push(new SynapseedItem(hash, {
                                description: message.substring(0, 60),
                                icon: 'git-commit',
                                tooltip: tt,
                            }));
                        }
                    }
                    if (commitItems.length > 0) {
                        items.push(sectionItem('Recent Commits', commitItems, 'history', true));
                    }
                }
            }

            // Intent summary
            if (intentRes.stdout && !intentRes.stdout.includes('error')) {
                const intentItems: SynapseedItem[] = [];
                for (const line of intentRes.stdout.split('\n')) {
                    const m = line.trim().match(/^(\w+):\s+(\d+)\s+commits?/i);
                    if (m) {
                        const icons: Record<string, string> = {
                            fix: 'wrench', feature: 'add', refactor: 'edit',
                            security: 'shield', docs: 'book', chore: 'gear',
                        };
                        intentItems.push(kvItem(m[1], `${m[2]} commits`, icons[m[1]] ?? 'tag'));
                    }
                }
                if (intentItems.length > 0) {
                    items.push(sectionItem('Commit Intent', intentItems, 'tag'));
                }
            }

            this.items = items.length > 0 ? items : [emptyItem('No git data')];
        } catch (e: unknown) {
            const msg = e instanceof Error ? e.message : String(e);
            log.error('Git load failed', e);
            this.items = [errorItem(msg)];
        }
        this._onDidChange.fire(undefined);
    }

    getTreeItem(el: SynapseedItem): vscode.TreeItem { return el; }
    getChildren(el?: SynapseedItem): SynapseedItem[] {
        return el?.children ?? (el ? [] : this.items);
    }
}
