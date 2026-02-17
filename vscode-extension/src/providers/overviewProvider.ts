import * as vscode from 'vscode';
import { runSynapseed, parseTextOutput } from '../cli';
import { log } from '../log';
import { CACHE_TTL, TIMEOUT } from '../constants';
import { SynapseedItem, kvItem, sectionItem, errorItem, emptyItem, loadingItem, progressItem } from '../items';

/**
 * Consolidated Overview provider — merges Status, Metrics, and Telemetry summary.
 * Single source of truth for project state and runtime metrics.
 */
export class OverviewProvider implements vscode.TreeDataProvider<SynapseedItem>, vscode.Disposable {
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
            const [statusRes, diagRes, telRes] = await Promise.all([
                runSynapseed(['status'], { cache: true, cacheTtlMs: CACHE_TTL.STATUS }),
                runSynapseed(['diagnose'], { cache: true, cacheTtlMs: CACHE_TTL.DEFAULT }),
                runSynapseed(
                    ['mcp', 'read', 'synapseed://telemetry/hotspots'],
                    { timeoutMs: TIMEOUT.TELEMETRY, cache: true, cacheTtlMs: CACHE_TTL.TELEMETRY },
                ),
            ]);

            const items: SynapseedItem[] = [];

            // ── Project Info (from status) ───────────────────────────
            if (statusRes.success || statusRes.stdout) {
                const sections = parseTextOutput(statusRes.stdout);
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
                            const tt = new vscode.MarkdownString(
                                `**${m[2]}** — Priority: ${m[3]}\n\nStatus: ${ok ? 'Active' : 'Failed'}`,
                            );
                            pluginItems.push(new SynapseedItem(m[2], {
                                description: `priority ${m[3]}`,
                                icon: ok ? 'pass-filled' : 'error',
                                tooltip: tt,
                                contextValue: 'synapseed.plugin',
                            }));
                        }
                    }
                    if (pluginItems.length > 0) {
                        items.push(sectionItem('Plugins', pluginItems, 'extensions'));
                    }
                }

                // Metrics from status output
                const metrics = sections.get('Metrics');
                if (metrics) {
                    const metricItems: SynapseedItem[] = [];
                    for (const [k, v] of metrics) {
                        if (k.startsWith('_item_')) { continue; }
                        let icon = 'symbol-number';
                        if (k.toLowerCase().includes('file')) { icon = 'files'; }
                        else if (k.toLowerCase().includes('symbol')) { icon = 'symbol-method'; }
                        else if (k.toLowerCase().includes('dlp') || k.toLowerCase().includes('block')) { icon = 'shield'; }
                        else if (k.toLowerCase().includes('command')) { icon = 'terminal'; }
                        else if (k.toLowerCase().includes('error')) { icon = 'error'; }
                        else if (k.toLowerCase().includes('event')) { icon = 'zap'; }
                        metricItems.push(kvItem(k, v, icon));
                    }
                    if (metricItems.length > 0) {
                        items.push(sectionItem('Runtime Metrics', metricItems, 'graph'));
                    }
                }
            }

            // ── Consistency score (from diagnose) ────────────────────
            if (diagRes.stdout) {
                const sec = parseTextOutput(diagRes.stdout);

                // Pipe-separated metrics
                const diagMetrics = sec.get('Metrics');
                if (diagMetrics) {
                    const extraMetrics: SynapseedItem[] = [];
                    for (const [k, v] of diagMetrics) {
                        if (!k.startsWith('_item_')) { continue; }
                        for (const part of v.split('|').map((p: string) => p.trim())) {
                            const kv = part.match(/^(\w[\w\s]*?):\s*(.+)$/);
                            if (kv && !items.find(i => i.label === kv[1].trim())) {
                                extraMetrics.push(kvItem(kv[1].trim(), kv[2].trim(), 'symbol-number'));
                            }
                        }
                    }
                    if (extraMetrics.length > 0) {
                        items.push(sectionItem('Index Metrics', extraMetrics, 'database'));
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
                        }
                    }
                }
            }

            // ── Telemetry summary (from MCP resource) ────────────────
            if (telRes.stdout?.trim()) {
                const jsonMatch = telRes.stdout.match(/(\{[\s\S]*\})/);
                if (jsonMatch) {
                    try {
                        const data = JSON.parse(jsonMatch[1]);
                        const telItems: SynapseedItem[] = [
                            kvItem('OTEL Spans', String(data.total_spans ?? 0), 'pulse'),
                            kvItem('Unique Locations', String(data.unique_locations ?? 0), 'symbol-method'),
                            kvItem('Buffer Usage', data.buffer_usage ?? '0%', 'database'),
                        ];
                        if ((data.hotspots ?? []).length > 0) {
                            const top = data.hotspots[0];
                            const sym = (top.key ?? '').split(':').pop() || top.key;
                            telItems.push(kvItem(
                                'Top Hotspot',
                                `${sym} (${top.avg_duration_ms?.toFixed(1) ?? '?'}ms avg)`,
                                'flame',
                            ));
                        }
                        items.push(sectionItem('Telemetry', telItems, 'broadcast'));
                    } catch (e: unknown) { log.warn('Failed to parse telemetry JSON', e); }
                }
            }

            // CLI latency
            items.push(kvItem('CLI Response', `${statusRes.durationMs}ms`, 'clock'));

            this.items = items.length > 0 ? items : [emptyItem('No data')];
        } catch (e: unknown) {
            const msg = e instanceof Error ? e.message : String(e);
            log.error('Overview load failed', e);
            this.items = [errorItem(msg)];
        }
        this._onDidChange.fire(undefined);
    }

    getTreeItem(el: SynapseedItem): vscode.TreeItem { return el; }
    getChildren(el?: SynapseedItem): SynapseedItem[] {
        return el?.children ?? (el ? [] : this.items);
    }
}
