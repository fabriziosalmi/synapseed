import * as vscode from 'vscode';
import { runSynapseed, runSynapseedJson } from '../cli';
import { SynapseedItem, kvItem, sectionItem, errorItem, emptyItem, loadingItem, gradeIcon, progressItem } from '../items';

interface ArchReport {
    score: number;
    grade: string;
    module_count: number;
    edge_count: number;
    avg_instability: number;
    avg_complexity: number;
    max_coupling: number;
    topological_density: number;
    modules: Array<{ name: string; ce: number; ca: number; instability: number; complexity: number }>;
    violations: Array<{ kind: string; message: string }>;
    recommendations: string[];
}

export class ArchitectureProvider implements vscode.TreeDataProvider<SynapseedItem> {
    private _onDidChange = new vscode.EventEmitter<SynapseedItem | undefined>();
    readonly onDidChangeTreeData = this._onDidChange.event;
    private items: SynapseedItem[] = [emptyItem('Click refresh to load')];

    refresh(): void { this.loadData(); }

    private async loadData(): Promise<void> {
        this.items = [loadingItem()];
        this._onDidChange.fire(undefined);

        try {
            const report = await runSynapseedJson<ArchReport>(['architect'], { cache: true, cacheTtlMs: 60_000 });

            if (!report) {
                // Fallback: parse text
                const result = await runSynapseed(['architect']);
                if (result.stdout) {
                    const items: SynapseedItem[] = [];
                    const sm = result.stdout.match(/Score:\s*(\d+)\/100\s*\(Grade:\s*(\w+)\)/);
                    if (sm) { items.push(kvItem('Grade', `${sm[2]} (${sm[1]}/100)`, gradeIcon(sm[2]))); }
                    this.items = items.length > 0 ? items : [emptyItem('No data')];
                } else {
                    this.items = [errorItem('Failed')];
                }
                this._onDidChange.fire(undefined);
                return;
            }

            const items: SynapseedItem[] = [];

            // Grade with rich tooltip
            const tooltip = new vscode.MarkdownString([
                `## Architecture Health: **${report.grade}**`,
                `| Metric | Value |`,
                `|--------|-------|`,
                `| Score | ${report.score}/100 |`,
                `| Modules | ${report.module_count} |`,
                `| Dependencies | ${report.edge_count} |`,
                `| Avg Instability | ${report.avg_instability.toFixed(2)} |`,
                `| Max Coupling | ${report.max_coupling} |`,
                `| Density | ${report.topological_density.toFixed(4)} |`,
            ].join('\n'));
            items.push(new SynapseedItem(`Grade: ${report.grade}`, {
                description: `${report.score}/100`,
                icon: gradeIcon(report.grade),
                tooltip,
            }));

            // Progress bars
            items.push(progressItem('Health', report.score, gradeIcon(report.grade)));

            // Summary
            items.push(kvItem('Modules', `${report.module_count}`, 'symbol-class'));
            items.push(kvItem('Dependencies', `${report.edge_count}`, 'git-merge'));
            items.push(kvItem('Avg Instability', report.avg_instability.toFixed(2), 'graph'));
            items.push(kvItem('Max Coupling', `${report.max_coupling}`, 'link'));

            // Violations
            if (report.violations.length > 0) {
                const vItems = report.violations.map((v: any) => {
                    const tt = new vscode.MarkdownString(`**${v.kind}**\n\n${v.message}`);
                    return new SynapseedItem(v.kind, { description: v.message, icon: 'error', tooltip: tt });
                });
                items.push(sectionItem('Violations', vItems, 'error'));
            } else {
                items.push(kvItem('Violations', '0', 'pass-filled'));
            }

            // Modules (by instability)
            if (report.modules.length > 0) {
                const sorted = [...report.modules].sort((a, b) => b.instability - a.instability).slice(0, 12);
                const modItems = sorted.map(m => {
                    const pct = Math.round(m.instability * 100);
                    const icon = pct > 80 ? 'flame' : pct > 50 ? 'warning' : 'pass';
                    const tt = new vscode.MarkdownString(
                        `**${m.name}**\n| Metric | Value |\n|--------|-------|\n| Instability | ${m.instability.toFixed(2)} |\n| Ce (efferent) | ${m.ce} |\n| Ca (afferent) | ${m.ca} |\n| Complexity | ${m.complexity} |`
                    );
                    return new SynapseedItem(m.name, {
                        description: `I=${m.instability.toFixed(2)} Ce=${m.ce} Ca=${m.ca}`,
                        icon,
                        tooltip: tt,
                    });
                });
                items.push(sectionItem('Modules (top instability)', modItems, 'symbol-module', true));
            }

            // Recommendations
            if (report.recommendations.length > 0) {
                const rItems = report.recommendations.map((r: string) =>
                    new SynapseedItem(r, { icon: 'lightbulb', tooltip: r })
                );
                items.push(sectionItem('Recommendations', rItems, 'lightbulb', true));
            }

            this.items = items;
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
