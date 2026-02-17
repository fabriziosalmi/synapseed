import * as vscode from 'vscode';
import * as path from 'path';
import { runSynapseed, getProjectRoot } from '../cli';
import { log } from '../log';
import { CACHE_TTL, TIMEOUT } from '../constants';
import { SynapseedItem, kvItem, sectionItem, errorItem, emptyItem, loadingItem, statusItem } from '../items';

/**
 * Consolidated Security provider — merges Security engines + Janitor proposals.
 * DLP, command sentinel, stats, clippy warnings, unused deps, fix proposals.
 */
export class SecurityProvider implements vscode.TreeDataProvider<SynapseedItem>, vscode.Disposable {
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

            // Scan real project config files (not a dummy string)
            const scanContent = await this.readConfigFileContent();

            const [scanRes, checkRes, statusRes, janitorRes] = await Promise.all([
                runSynapseed(['scan', '--content', scanContent], { cache: true, cacheTtlMs: CACHE_TTL.DEFAULT * 2 }),
                runSynapseed(['check', 'cargo test'], { cache: true, cacheTtlMs: CACHE_TTL.DEFAULT * 2 }),
                runSynapseed(['status'], { cache: true, cacheTtlMs: CACHE_TTL.STATUS }),
                runSynapseed(['janitor'], { timeoutMs: TIMEOUT.LONG * 2, cache: true, cacheTtlMs: CACHE_TTL.ARCHITECTURE }),
            ]);

            // ── Security Engines ─────────────────────────────────────
            if (scanRes.stdout) {
                const ok = scanRes.stdout.includes('CLEAN');
                items.push(statusItem('DLP Engine', ok, 'Active', 'Alert'));
            }
            if (checkRes.stdout) {
                const ok = checkRes.stdout.includes('ALLOWED');
                items.push(statusItem('Command Sentinel', ok, 'Active', 'Restricted'));
            }

            // Stats from status
            if (statusRes.stdout) {
                const statItems: SynapseedItem[] = [];
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
                        statItems.push(kvItem(name, m[1], isAlert ? 'warning' : icon));
                    }
                }
                if (statItems.length > 0) {
                    items.push(sectionItem('Security Stats', statItems, 'shield'));
                }
            }

            // ── Janitor (clippy + unused deps + proposals) ───────────
            if (janitorRes.stdout) {
                const text = janitorRes.stdout;

                if (text.includes('background') || text.includes('Background')) {
                    items.push(kvItem('Janitor', 'Scan running...', 'sync~spin'));
                } else if (text.includes('No findings') || text.includes('clean')) {
                    items.push(kvItem('Janitor', 'All clean', 'pass-filled'));
                } else {
                    const clippyItems: SynapseedItem[] = [];
                    const depItems: SynapseedItem[] = [];
                    const proposalItems: SynapseedItem[] = [];

                    for (const line of text.split('\n')) {
                        const t = line.trim();
                        if (!t || t.startsWith('===') || t.startsWith('---')) { continue; }

                        if (t.includes('clippy') || t.includes('warning[')) {
                            clippyItems.push(new SynapseedItem(t.substring(0, 100), {
                                icon: 'warning',
                                tooltip: t,
                                contextValue: 'synapseed.clippy',
                            }));
                        } else if (t.includes('unused') && t.includes('dep')) {
                            depItems.push(new SynapseedItem(t.substring(0, 80), {
                                icon: 'trash',
                                tooltip: t,
                                contextValue: 'synapseed.unusedDep',
                            }));
                        } else if (t.includes('proposal') || t.match(/^[0-9a-f-]{36}/)) {
                            const idMatch = t.match(/([0-9a-f-]{36})/);
                            proposalItems.push(new SynapseedItem(t.substring(0, 80), {
                                icon: 'tools',
                                tooltip: t,
                                contextValue: idMatch ? 'synapseed.proposal' : undefined,
                            }));
                        }
                    }

                    if (clippyItems.length > 0) { items.push(sectionItem('Clippy Warnings', clippyItems, 'warning')); }
                    if (depItems.length > 0) { items.push(sectionItem('Unused Dependencies', depItems, 'trash')); }
                    if (proposalItems.length > 0) { items.push(sectionItem('Fix Proposals', proposalItems, 'tools')); }
                }
            }

            // Defense-in-depth tooltip
            const summary = new vscode.MarkdownString([
                '## Security Model',
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
        } catch (e: unknown) {
            const msg = e instanceof Error ? e.message : String(e);
            log.error('Security load failed', e);
            this.items = [errorItem(msg)];
        }
        this._onDidChange.fire(undefined);
    }

    /** Read the first available config file from the workspace for real DLP scanning. */
    private async readConfigFileContent(): Promise<string> {
        const root = getProjectRoot();
        if (!root) { return 'no workspace open'; }

        const candidates = ['.env', '.env.local', 'Cargo.toml', 'package.json', '.synapseed/dna.yaml'];
        for (const name of candidates) {
            const uri = vscode.Uri.file(path.join(root, name));
            try {
                const stat = await vscode.workspace.fs.stat(uri);
                if (stat.type === vscode.FileType.File && stat.size < 64 * 1024) {
                    const bytes = await vscode.workspace.fs.readFile(uri);
                    return Buffer.from(bytes).toString('utf8');
                }
            } catch { /* file doesn't exist, try next */ }
        }
        return 'no config files found for scanning';
    }

    getTreeItem(el: SynapseedItem): vscode.TreeItem { return el; }
    getChildren(el?: SynapseedItem): SynapseedItem[] {
        return el?.children ?? (el ? [] : this.items);
    }
}
