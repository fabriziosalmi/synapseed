import * as vscode from 'vscode';
import { runSynapseed } from '../cli';
import { SynapseedItem, kvItem, sectionItem, emptyItem, errorItem } from '../items';

/**
 * OTEL Telemetry view — shows runtime hotspots received via the OTLP gRPC receiver.
 * Data source: `synapseed mcp read synapseed://telemetry/hotspots`
 */
export class TelemetryProvider implements vscode.TreeDataProvider<SynapseedItem> {
    private _onDidChange = new vscode.EventEmitter<SynapseedItem | undefined>();
    readonly onDidChangeTreeData = this._onDidChange.event;

    private items: SynapseedItem[] = [];
    private loading = false;

    refresh(): void {
        this._onDidChange.fire(undefined);
    }

    getTreeItem(element: SynapseedItem): vscode.TreeItem {
        return element;
    }

    getChildren(element?: SynapseedItem): SynapseedItem[] {
        if (element?.children) {
            return element.children;
        }
        if (!element) {
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
            // Read the hotspots resource via MCP
            const result = await runSynapseed(
                ['mcp', 'read', 'synapseed://telemetry/hotspots'],
                15000,
            );

            if (!result.stdout || result.stdout.trim().length === 0) {
                this.items = [emptyItem('No telemetry data yet')];
                this._onDidChange.fire(undefined);
                return;
            }

            // Parse JSON from output
            const jsonMatch = result.stdout.match(/(\{[\s\S]*\})/);
            if (!jsonMatch) {
                this.items = [emptyItem('No telemetry data yet')];
                this._onDidChange.fire(undefined);
                return;
            }

            const data = JSON.parse(jsonMatch[1]) as TelemetryHotspots;

            // ── Store Stats ──────────────────────────────────────
            const statsItems: SynapseedItem[] = [
                kvItem('Total Spans', String(data.total_spans ?? 0), 'pulse'),
                kvItem('Unique Locations', String(data.unique_locations ?? 0), 'symbol-method'),
                kvItem('Buffer Usage', data.buffer_usage ?? '0%', 'database'),
            ];
            const statsSection = sectionItem('Store Stats', statsItems, 'graph');

            // ── Hotspots ─────────────────────────────────────────
            const hotspotItems: SynapseedItem[] = (data.hotspots ?? []).map((h, i) => {
                const [filePath, symbol] = (h.key ?? '').split(':');
                const durationColor = h.avg_duration_ms > 200 ? 'error'
                    : h.avg_duration_ms > 50 ? 'warning'
                        : 'pass';

                const children: SynapseedItem[] = [
                    kvItem('Calls', String(h.call_count), 'history'),
                    kvItem('Avg Duration', `${h.avg_duration_ms.toFixed(1)}ms`, 'watch'),
                    kvItem('Max Duration', `${h.max_duration_ms.toFixed(1)}ms`, 'flame'),
                    kvItem('P95 Duration', `${h.p95_duration_ms.toFixed(1)}ms`, 'graph-line'),
                    kvItem('Last Seen', h.last_seen ?? 'N/A', 'calendar'),
                ];

                const item = new SynapseedItem(
                    `#${i + 1} ${symbol || h.key}`,
                    `${h.avg_duration_ms.toFixed(1)}ms avg · ${h.call_count} calls`,
                    vscode.TreeItemCollapsibleState.Collapsed,
                    children,
                    durationColor,
                );

                // Click to navigate to file
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

            // ── OTLP Receiver ────────────────────────────────────
            const receiverItem = kvItem('OTLP Receiver', '127.0.0.1:4317', 'broadcast');

            this.items = [statsSection, hotspotsSection, receiverItem];
        } catch (err: any) {
            this.items = [errorItem(err.message ?? 'Failed to load telemetry')];
        }

        this._onDidChange.fire(undefined);
    }
}

interface TelemetryHotspot {
    key: string;
    call_count: number;
    avg_duration_ms: number;
    max_duration_ms: number;
    p95_duration_ms: number;
    last_seen: string;
}

interface TelemetryHotspots {
    total_spans: number;
    unique_locations: number;
    buffer_usage: string;
    hotspots: TelemetryHotspot[];
}
