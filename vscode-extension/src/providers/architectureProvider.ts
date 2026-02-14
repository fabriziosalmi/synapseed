import * as vscode from 'vscode';
import { runSynapseed, runSynapseedJson } from '../cli';
import { SynapseedItem, kvItem, sectionItem, errorItem, emptyItem } from '../items';

interface ArchReport {
    score: number;
    grade: string;
    module_count: number;
    edge_count: number;
    avg_instability: number;
    avg_complexity: number;
    max_coupling: number;
    topological_density: number;
    modules: Array<{
        name: string;
        ce: number;
        ca: number;
        instability: number;
        complexity: number;
    }>;
    violations: Array<{
        kind: string;
        message: string;
    }>;
    recommendations: string[];
}

/**
 * Architecture Health view — grade, modules, coupling, violations.
 */
export class ArchitectureProvider implements vscode.TreeDataProvider<SynapseedItem> {
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
            const report = await runSynapseedJson<ArchReport>(['architect']);

            if (!report) {
                // Fallback: parse text output
                const result = await runSynapseed(['architect']);
                if (result.stdout) {
                    const scoreMatch = result.stdout.match(/Score:\s*(\d+)\/100\s*\(Grade:\s*(\w+)\)/);
                    const modulesMatch = result.stdout.match(/Modules:\s*(\d+)/);
                    const edgesMatch = result.stdout.match(/Edges:\s*(\d+)/);
                    const violationsMatch = result.stdout.match(/Violations:\s*(\d+)/);

                    const items: SynapseedItem[] = [];
                    if (scoreMatch) {
                        const grade = scoreMatch[2];
                        const icon = grade === 'A' ? 'pass' : grade === 'B' ? 'info' : grade === 'C' ? 'warning' : 'error';
                        items.push(kvItem('Grade', `${grade} (${scoreMatch[1]}/100)`, icon));
                    }
                    if (modulesMatch) { items.push(kvItem('Modules', modulesMatch[1], 'symbol-class')); }
                    if (edgesMatch) { items.push(kvItem('Dependencies', edgesMatch[1], 'git-merge')); }
                    if (violationsMatch) {
                        const v = parseInt(violationsMatch[1]);
                        items.push(kvItem('Violations', violationsMatch[1], v === 0 ? 'pass' : 'error'));
                    }
                    this.items = items.length > 0 ? items : [emptyItem('No architecture data')];
                } else {
                    this.items = [errorItem('Failed to get architecture report')];
                }
                this._onDidChange.fire(undefined);
                return;
            }

            const items: SynapseedItem[] = [];

            // Grade
            const gradeIcon = report.grade === 'A' ? 'pass' :
                report.grade === 'B' ? 'info' :
                    report.grade === 'C' ? 'warning' : 'error';
            items.push(kvItem('Grade', `${report.grade} (${report.score}/100)`, gradeIcon));

            // Summary metrics
            items.push(kvItem('Modules', `${report.module_count}`, 'symbol-class'));
            items.push(kvItem('Dependencies', `${report.edge_count}`, 'git-merge'));
            items.push(kvItem('Avg Instability', report.avg_instability.toFixed(2), 'graph'));
            items.push(kvItem('Avg Complexity', report.avg_complexity.toFixed(2), 'symbol-number'));
            items.push(kvItem('Max Coupling', `${report.max_coupling}`, 'link'));
            items.push(kvItem('Density', report.topological_density.toFixed(4), 'graph-scatter'));

            // Violations
            if (report.violations.length > 0) {
                const violationItems = report.violations.map((v: any) =>
                    kvItem(v.kind, v.message, 'error')
                );
                items.push(sectionItem('Violations', violationItems, 'error'));
            } else {
                items.push(kvItem('Violations', '0', 'pass'));
            }

            // Top modules by instability (if any)
            if (report.modules.length > 0) {
                const sorted = [...report.modules].sort((a, b) => b.instability - a.instability).slice(0, 10);
                const modItems = sorted.map(m =>
                    kvItem(m.name, `I=${m.instability.toFixed(2)} Ce=${m.ce} Ca=${m.ca}`, 'symbol-module')
                );
                items.push(sectionItem('Modules (by instability)', modItems, 'symbol-module'));
            }

            // Recommendations
            if (report.recommendations.length > 0) {
                const recItems = report.recommendations.map((r: any) =>
                    new SynapseedItem(r, undefined, vscode.TreeItemCollapsibleState.None, undefined, 'lightbulb')
                );
                items.push(sectionItem('Recommendations', recItems, 'lightbulb'));
            }

            this.items = items;
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
