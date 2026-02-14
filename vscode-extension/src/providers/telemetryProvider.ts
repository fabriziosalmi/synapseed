import * as vscode from 'vscode';
import { runSynapseed } from '../cli';
import { SynapseedItem, kvItem, sectionItem, emptyItem, errorItem, loadingItem } from '../items';

interface TelemetryHotspot {
    key: string;
    call_count: number;
    avg_duration_ms: number;
    max_duration_ms: number;
    p95_duration_ms: number;
    last_seen: string;
}

interface TelemetryData {
    total_spans: number;
    unique_locations: number;
    buffer_usage: string;
    hotspots: TelemetryHotspot[];
}

export class TelemetryProvider implements vscode.TreeDataProvider<SynapseedItem> {
    private _onDidChange = new vscode.EventEmitter<SynapseedItem | undefined>();
    readonly onDidChangeTreeData = this._onDidChange.event;
    private items: SynapseedItem[] = [];
    private loading = false;

    refresh(): void { this._onDidChange.fire(undefined); }

    getTreeItem(el: SynapseedItem): vscode.TreeItem { return el; }

    getChildren(el?: SynapseedItem): SynapseedItem[] {
        if (el?.children) { return el.children; }
        if (!el) {
            if (!this.loading) {
                this.loading = true;
                this.loadData().then(() => { this.loading = false; });
            }
            return this.items;
        }
        return [];
    }

    private async loadData(): Promise<void> {
        try {
            const result = await runSynapseed(
                ['mcp', 'read', 'synapseed://telemetry/hotspots'],
                { timeoutMs: 15_000 },
            );

            if (!result.stdout?.trim()) {
                this.items = [emptyItem('No telemetry data yet')];
                this._onDidChange.fire(undefined);
                return;
            }

            const jsonMatch = result.stdout.match(/(\{[\s\S]*\})/);
            if (!jsonMatch) {
                this.items = [emptyItem('No telemetry data yet')];
                this._onDidChange.fire(undefined);
                return;
            }

            const data: TelemetryData = JSON.parse(jsonMatch[1]);

            // Store stats
            const statsItems: SynapseedItem[] = [
                kvItem('Total Spans', String(data.total_spans ?? 0), 'pulse'),
                kvItem('Unique Locations', String(data.unique_locations ?? 0), 'symbol-method'),
                kvItem('Buffer Usage', data.buffer_usage ?? '0%', 'database'),
            ];
            const statsSection = sectionItem('Store Stats', statsItems, 'graph');

            // Hotspots with rich tooltips
            const hotspotItems: SynapseedItem[] = (data.hotspots ?? []).map((h, i) => {
                const [filePath, symbol] = (h.key ?? '').split(':');
                const durationIcon = h.avg_duration_ms > 200 ? 'flame' :
                    h.avg_duration_ms > 50 ? 'warning' : 'pass';

                const children: SynapseedItem[] = [
                    kvItem('Calls', String(h.call_count), 'history'),
                    kvItem('Avg Duration', `${h.avg_duration_ms.toFixed(1)}ms`, 'watch'),
                    kvItem('Max Duration', `${h.max_duration_ms.toFixed(1)}ms`, 'flame'),
                    kvItem('P95 Duration', `${h.p95_duration_ms.toFixed(1)}ms`, 'graph-line'),
                    kvItem('Last Seen', h.last_seen ?? 'N/A', 'calendar'),
                ];

                const tooltip = new vscode.MarkdownString([
                    `## #${i + 1} ${symbol || h.key}`,
                    `| Metric | Value |`,
                    `|--------|-------|`,
                    `| Calls | ${h.call_count} |`,
                    `| Avg | ${h.avg_duration_ms.toFixed(1)}ms |`,
                    `| Max | ${h.max_duration_ms.toFixed(1)}ms |`,
                    `| P95 | ${h.p95_duration_ms.toFixed(1)}ms |`,
                ].join('\n'));

                const item = new SynapseedItem(`#${i + 1} ${symbol || h.key}`, {
                    description: `${h.avg_duration_ms.toFixed(1)}ms avg · ${h.call_count} calls`,
                    icon: durationIcon,
                    state: vscode.TreeItemCollapsibleState.Collapsed,
                    children,
                    tooltip,
                });

                if (filePath) {
                    item.command = {
                        command: 'vscode.open',
                        title: 'Open File',
                        arguments: [vscode.Uri.file(filePath)],
                    };
                }

                return item;
            });

            const hotspotsSection = hotspotItems.length > 0
                ? sectionItem('Hotspots (by avg duration)', hotspotItems, 'flame')
                : emptyItem('No hotspots detected');

            const receiverItem = kvItem('OTLP Receiver', '127.0.0.1:4317', 'broadcast');

            this.items = [statsSection, hotspotsSection, receiverItem];
        } catch (err: any) {
            this.items = [errorItem(err.message ?? 'Failed')];
        }
        this._onDidChange.fire(undefined);
    }
}
