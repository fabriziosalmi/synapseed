import * as vscode from 'vscode';
import { runSynapseed, runSynapseedJson, parseTextOutput } from '../cli';
import { log } from '../log';
import { CACHE_TTL, TIMEOUT, MAX_MODULES_DISPLAY } from '../constants';
import { ArchitectReport } from '../types';
import { SynapseedItem, kvItem, sectionItem, errorItem, emptyItem, loadingItem, progressItem, gradeIcon } from '../items';

/**
 * Consolidated Code Quality provider — merges Architecture Health and Consistency.
 * Shows grade, modules, instability, consistency score, doc drift, recommendations.
 */
export class CodeQualityProvider implements vscode.TreeDataProvider<SynapseedItem>, vscode.Disposable {
    private _onDidChange = new vscode.EventEmitter<SynapseedItem | undefined>();
    readonly onDidChangeTreeData = this._onDidChange.event;
    private items: SynapseedItem[] = [emptyItem('Click refresh to load')];

    dispose(): void { this._onDidChange.dispose(); }
    refresh(): void { this.loadData(); }

    private async loadData(): Promise<void> {
        // Keep previous items visible during refresh (no flash).
        // Only show loading indicator if we have no data yet.
        if (this.items.length <= 1) {
            this.items = [loadingItem()];
            this._onDidChange.fire(undefined);
        }

        try {
            const [archReport, diagRes, oracleRes] = await Promise.all([
                runSynapseedJson<ArchitectReport>(['architect'], { cache: true, cacheTtlMs: CACHE_TTL.ARCHITECTURE }),
                runSynapseed(['diagnose'], { cache: true, cacheTtlMs: CACHE_TTL.DEFAULT }),
                runSynapseed(['oracle'], { timeoutMs: TIMEOUT.LONG }),
            ]);

            const items: SynapseedItem[] = [];

            // ── Architecture Grade ───────────────────────────────────
            if (archReport) {
                // Grade with rich tooltip
                const tooltip = new vscode.MarkdownString([
                    `## Architecture Health: **${archReport.grade}**`,
                    `| Metric | Value |`,
                    `|--------|-------|`,
                    `| Score | ${archReport.score}/100 |`,
                    `| Modules | ${archReport.module_count} |`,
                    `| Dependencies | ${archReport.edge_count} |`,
                    `| Avg Instability | ${archReport.avg_instability.toFixed(2)} |`,
                    `| Max Coupling | ${archReport.max_coupling} |`,
                    `| Density | ${archReport.topological_density.toFixed(4)} |`,
                ].join('\n'));
                items.push(new SynapseedItem(`Grade: ${archReport.grade}`, {
                    description: `${archReport.score}/100`,
                    icon: gradeIcon(archReport.grade),
                    tooltip,
                }));

                items.push(progressItem('Health', archReport.score, gradeIcon(archReport.grade)));
                items.push(kvItem('Modules', `${archReport.module_count}`, 'symbol-class'));
                items.push(kvItem('Dependencies', `${archReport.edge_count}`, 'git-merge'));
                items.push(kvItem('Avg Instability', archReport.avg_instability.toFixed(2), 'graph'));
                items.push(kvItem('Max Coupling', `${archReport.max_coupling}`, 'link'));

                // Violations
                if (archReport.violations.length > 0) {
                    const vItems = archReport.violations.map(v => {
                        const tt = new vscode.MarkdownString(
                            `**${v.rule}** (${v.severity})\n\n${v.description}\n\n**Suggestion:** ${v.suggestion}\n\n**Modules:** ${v.modules.join(', ')}`,
                        );
                        const icon = v.severity === 'critical' ? 'error' : v.severity === 'error' ? 'error' : 'warning';
                        return new SynapseedItem(v.rule, { description: v.description, icon, tooltip: tt });
                    });
                    items.push(sectionItem('Violations', vItems, 'error'));
                } else {
                    items.push(kvItem('Violations', '0', 'pass-filled'));
                }

                // Modules (by instability)
                if (archReport.modules.length > 0) {
                    const sorted = [...archReport.modules].sort((a, b) => b.instability - a.instability).slice(0, MAX_MODULES_DISPLAY);
                    const modItems = sorted.map(m => {
                        const pct = Math.round(m.instability * 100);
                        const icon = pct > 80 ? 'flame' : pct > 50 ? 'warning' : 'pass';
                        const tt = new vscode.MarkdownString(
                            `**${m.module_name}**\n| Metric | Value |\n|--------|-------|\n| Instability | ${m.instability.toFixed(2)} |\n| Ce (efferent) | ${m.efferent_coupling} |\n| Ca (afferent) | ${m.afferent_coupling} |\n| Complexity | ${m.approx_complexity} |\n| Fan-in | ${m.fan_in} |\n| Fan-out | ${m.fan_out} |`,
                        );
                        return new SynapseedItem(m.module_name, {
                            description: `I=${m.instability.toFixed(2)} Ce=${m.efferent_coupling} Ca=${m.afferent_coupling}`,
                            icon,
                            tooltip: tt,
                        });
                    });
                    items.push(sectionItem('Modules (top instability)', modItems, 'symbol-module', true));
                }

                // Recommendations
                if (archReport.recommendations.length > 0) {
                    const rItems = archReport.recommendations.map(r => {
                        const tt = new vscode.MarkdownString(
                            `**P${r.priority}** [${r.category}]\n\n${r.action}\n\n**Modules:** ${r.modules.join(', ')}`,
                        );
                        return new SynapseedItem(r.action.substring(0, 80), { icon: 'lightbulb', tooltip: tt, description: `P${r.priority} ${r.category}` });
                    });
                    items.push(sectionItem('Recommendations', rItems, 'lightbulb', true));
                }
            } else {
                // Fallback: parse text
                const result = await runSynapseed(['architect']);
                if (result.stdout) {
                    const sm = result.stdout.match(/Score:\s*(\d+)\/100\s*\(Grade:\s*(\w+)\)/);
                    if (sm) { items.push(kvItem('Grade', `${sm[2]} (${sm[1]}/100)`, gradeIcon(sm[2]))); }
                }
            }

            // ── Consistency (from diagnose) ──────────────────────────
            if (diagRes.stdout) {
                const sec = parseTextOutput(diagRes.stdout);
                const consistency = sec.get('Consistency');
                if (consistency) {
                    for (const [k, v] of consistency) {
                        if (!k.startsWith('_item_')) { continue; }
                        const m = v.match(/(\d+)\s+passed.*?(\d+)\s+failed.*?score:\s*(\d+)%/);
                        if (m) {
                            const pct = parseInt(m[3]);
                            items.push(progressItem('Consistency Score', pct, pct >= 90 ? 'pass-filled' : 'warning'));
                            items.push(kvItem('Checks Passed', m[1], 'check-all'));
                            const fails = parseInt(m[2]);
                            items.push(kvItem('Checks Failed', m[2], fails === 0 ? 'check-all' : 'error'));
                        }
                    }
                }
            }

            // ── Documentation Drift (from oracle) ────────────────────
            if (oracleRes.stdout) {
                const text = oracleRes.stdout;
                if (text.includes('no drift') || text.includes('No drift') || text.includes('0 changes')) {
                    items.push(kvItem('Doc Drift', 'None detected', 'pass-filled'));
                } else {
                    const changes = text.split('\n')
                        .filter((l: string) => l.trim() && !l.startsWith('==='))
                        .slice(0, 10)
                        .map((l: string) => new SynapseedItem(l.trim(), { icon: 'warning' }));
                    if (changes.length > 0) {
                        items.push(sectionItem('Doc Drift', changes, 'warning'));
                    }
                }
            }

            this.items = items.length > 0 ? items : [emptyItem('No data')];
        } catch (e: unknown) {
            const msg = e instanceof Error ? e.message : String(e);
            log.error('Code quality load failed', e);
            this.items = [errorItem(msg)];
        }
        this._onDidChange.fire(undefined);
    }

    getTreeItem(el: SynapseedItem): vscode.TreeItem { return el; }
    getChildren(el?: SynapseedItem): SynapseedItem[] {
        return el?.children ?? (el ? [] : this.items);
    }
}
