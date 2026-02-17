import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';
import { getProjectRoot } from './cli';
import { log } from './log';
import { escapeHtml, getNonce } from './html';
import { COLORS } from './constants';
import { BenchmarkMessage, BenchmarkFileData } from './types';

/**
 * BenchmarkPanel — webview panel for visualizing benchmark results.
 * v3: typed data, CSP nonce, structured logging, error handling.
 */
export class BenchmarkPanel {
    private static instance: BenchmarkPanel | undefined;
    private readonly panel: vscode.WebviewPanel;

    static show(extensionUri: vscode.Uri) {
        if (BenchmarkPanel.instance) {
            BenchmarkPanel.instance.panel.reveal();
            BenchmarkPanel.instance.refresh();
            return;
        }
        new BenchmarkPanel(extensionUri);
    }

    private constructor(extensionUri: vscode.Uri) {
        this.panel = vscode.window.createWebviewPanel(
            'synapseed.benchmark',
            'SYNAPSEED Benchmarks',
            vscode.ViewColumn.One,
            { enableScripts: true, retainContextWhenHidden: true },
        );
        BenchmarkPanel.instance = this;
        this.panel.onDidDispose(() => { BenchmarkPanel.instance = undefined; });

        this.panel.webview.onDidReceiveMessage(async (msg: BenchmarkMessage) => {
            try {
                switch (msg.type) {
                    case 'refresh':
                        this.refresh();
                        break;
                    case 'openFile':
                        if (msg.path) {
                            const doc = await vscode.workspace.openTextDocument(msg.path);
                            await vscode.window.showTextDocument(doc);
                        }
                        break;
                    case 'import':
                        await this.importFile();
                        break;
                }
            } catch (err) {
                log.warn('Benchmark message handler error', err);
            }
        });

        this.refresh();
    }

    private async importFile() {
        const uris = await vscode.window.showOpenDialog({
            canSelectFiles: true,
            canSelectMany: true,
            filters: { 'JSON': ['json'] },
            openLabel: 'Import Benchmark Results',
        });
        if (uris?.length) {
            const resultsDir = this.getResultsDir();
            if (!resultsDir) {
                vscode.window.showWarningMessage('benchmark/results/ directory not found');
                return;
            }
            for (const uri of uris) {
                try {
                    const dest = vscode.Uri.file(path.join(resultsDir, path.basename(uri.fsPath)));
                    await vscode.workspace.fs.copy(uri, dest, { overwrite: true });
                } catch (err) {
                    log.warn(`Failed to import ${uri.fsPath}`, err);
                }
            }
            this.refresh();
        }
    }

    private getResultsDir(): string | undefined {
        const root = getProjectRoot();
        if (!root) { return undefined; }
        const dir = path.join(root, 'benchmark', 'results');
        if (!fs.existsSync(dir)) { return undefined; }
        return dir;
    }

    private loadResults(): BenchmarkFile[] {
        const dir = this.getResultsDir();
        if (!dir) { return []; }

        const files = fs.readdirSync(dir)
            .filter(f => f.endsWith('.json') && !f.startsWith('.'))
            .sort()
            .reverse();

        const results: BenchmarkFile[] = [];
        for (const f of files) {
            try {
                const raw = fs.readFileSync(path.join(dir, f), 'utf-8');
                const data: BenchmarkFileData = JSON.parse(raw);
                const benchType = this.detectBenchType(f, data);
                results.push({ filename: f, path: path.join(dir, f), type: benchType, data });
            } catch (err) {
                log.warn(`Skipped malformed benchmark file: ${f}`, err);
            }
        }
        return results;
    }

    private detectBenchType(filename: string, data: BenchmarkFileData): BenchType {
        if (filename.startsWith('coding_') || data.metadata?.benchmark_type === 'Coding' || ('conditions' in data)) {
            return 'coding';
        }
        if (filename.startsWith('grounding_') || data.metadata?.benchmark_type === 'Grounding') {
            return 'grounding';
        }
        if (filename.startsWith('search_') || data.metadata?.benchmark_type === 'Search') {
            return 'search';
        }
        if (filename.includes('bench_engine')) {
            return 'engine';
        }
        return 'unknown';
    }

    private refresh() {
        const results = this.loadResults();
        this.panel.webview.html = this.getHtml(results);
    }

    private getHtml(files: BenchmarkFile[]): string {
        const nonce = getNonce();
        const esc = escapeHtml;
        const coding = files.filter(f => f.type === 'coding');
        const grounding = files.filter(f => f.type === 'grounding');
        const search = files.filter(f => f.type === 'search');
        const engine = files.filter(f => f.type === 'engine');
        const other = files.filter(f => f.type === 'unknown');

        const totalFiles = files.length;

        return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'nonce-${nonce}'; script-src 'nonce-${nonce}';">
<style nonce="${nonce}">
    *{box-sizing:border-box;margin:0;padding:0;}
    body{font-family:var(--vscode-font-family);color:var(--vscode-foreground);background:var(--vscode-editor-background);line-height:1.5;}
    .top-bar{display:flex;align-items:center;justify-content:space-between;padding:14px 24px;position:sticky;top:0;z-index:10;background:var(--vscode-editor-background);border-bottom:1px solid var(--vscode-panel-border);}
    .top-bar h1{font-size:18px;font-weight:700;letter-spacing:-0.3px;}
    .top-bar .label{font-size:11px;text-transform:uppercase;letter-spacing:0.8px;opacity:0.5;margin-bottom:2px;}
    .top-actions{display:flex;gap:8px;}
    .btn{padding:6px 14px;border:1px solid var(--vscode-panel-border);border-radius:4px;background:var(--vscode-button-secondaryBackground);color:var(--vscode-button-secondaryForeground);cursor:pointer;font-size:12px;font-weight:500;transition:opacity .15s;}
    .btn:hover{opacity:.8;}
    .btn.primary{background:var(--vscode-button-background);color:var(--vscode-button-foreground);border:none;}
    .tabs{display:flex;gap:0;padding:0 24px;position:sticky;top:52px;z-index:9;background:var(--vscode-editor-background);border-bottom:1px solid var(--vscode-panel-border);}
    .tab{padding:10px 18px;cursor:pointer;font-size:12px;font-weight:500;border-bottom:2px solid transparent;color:var(--vscode-descriptionForeground);transition:all .15s;text-transform:uppercase;letter-spacing:0.3px;}
    .tab:hover{color:var(--vscode-foreground);}
    .tab.active{color:var(--vscode-focusBorder);border-bottom-color:var(--vscode-focusBorder);}
    .tc{display:none;padding:24px;}
    .tc.active{display:block;}
    .cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(140px,1fr));gap:12px;margin-bottom:24px;}
    .card{border:1px solid var(--vscode-panel-border);border-radius:8px;padding:16px;text-align:center;position:relative;overflow:hidden;transition:transform .15s,box-shadow .15s;}
    .card:hover{transform:translateY(-1px);box-shadow:0 2px 8px rgba(0,0,0,.15);}
    .card .val{font-size:32px;font-weight:800;line-height:1.2;}
    .card .lbl{font-size:11px;text-transform:uppercase;letter-spacing:0.5px;opacity:.5;margin-top:2px;}
    .card .bar{position:absolute;bottom:0;left:0;height:2px;}
    .section{margin-bottom:24px;}
    .section h2{font-size:14px;font-weight:600;margin-bottom:10px;display:flex;align-items:center;gap:8px;text-transform:uppercase;letter-spacing:0.3px;opacity:.8;}
    .section h2 .dot{width:6px;height:6px;border-radius:50%;display:inline-block;}
    table{width:100%;border-collapse:collapse;font-size:12px;}
    th{text-align:left;padding:8px 10px;border-bottom:2px solid var(--vscode-panel-border);font-weight:600;font-size:10px;text-transform:uppercase;letter-spacing:0.5px;opacity:.5;}
    td{padding:7px 10px;border-bottom:1px solid var(--vscode-panel-border);}
    tr:hover td{background:var(--vscode-list-hoverBackground);}
    tr.click{cursor:pointer;}
    .bar-chart{display:flex;flex-direction:column;gap:6px;}
    .bar-row{display:flex;align-items:center;gap:8px;}
    .bar-lbl{width:130px;font-size:11px;text-align:right;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;}
    .bar-track{flex:1;height:18px;background:var(--vscode-panel-border);border-radius:3px;overflow:hidden;position:relative;}
    .bar-fill{height:100%;border-radius:3px;transition:width .6s ease;}
    .bar-val{position:absolute;right:5px;top:1px;font-size:10px;font-weight:600;}
    .detail{padding:12px;background:var(--vscode-textBlockQuote-background);border-radius:6px;margin:8px 0;font-size:12px;}
    .detail .row{display:flex;justify-content:space-between;padding:4px 0;border-bottom:1px solid var(--vscode-panel-border);}
    .detail .row:last-child{border-bottom:none;}
    .two-col{display:grid;grid-template-columns:1fr 1fr;gap:24px;}
    @media(max-width:600px){.two-col{grid-template-columns:1fr;}}
    .empty-state{text-align:center;padding:60px 24px;opacity:.5;}
    .empty-state h2{font-size:20px;margin-bottom:8px;opacity:.8;}
    .empty-state p{font-size:13px;}
    .file-item{padding:8px 12px;border-left:3px solid var(--vscode-focusBorder);margin-bottom:6px;border-radius:0 4px 4px 0;background:var(--vscode-textBlockQuote-background);font-size:12px;cursor:pointer;display:flex;justify-content:space-between;align-items:center;}
    .file-item:hover{opacity:.85;}
    .file-item .fname{font-weight:600;}
    .file-item .fmeta{opacity:.5;font-size:11px;}
    .delta-pos{color:${COLORS.PASS};font-weight:700;}
    .delta-neg{color:${COLORS.ERROR};font-weight:700;}
    .delta-zero{opacity:.5;}
    .badge{display:inline-block;padding:1px 6px;border-radius:3px;font-size:10px;font-weight:600;margin-left:4px;}
    .badge-coding{background:rgba(33,150,243,.15);color:${COLORS.INFO};}
    .badge-grounding{background:rgba(76,175,80,.15);color:${COLORS.PASS};}
    .badge-search{background:rgba(124,77,255,.15);color:${COLORS.PURPLE};}
    .badge-engine{background:rgba(255,152,0,.15);color:${COLORS.WARN};}
    .expand-btn{cursor:pointer;font-size:10px;opacity:.5;text-transform:uppercase;padding:4px 8px;border:1px solid var(--vscode-panel-border);border-radius:3px;background:transparent;color:var(--vscode-foreground);}
    .expand-btn:hover{opacity:.8;}
    .task-detail{display:none;padding:12px;margin:4px 0 8px 0;background:var(--vscode-textBlockQuote-background);border-radius:6px;font-size:11px;}
    .task-detail.visible{display:block;}
</style>
</head>
<body>
    <div class="top-bar">
        <div>
            <div class="label">SYNAPSEED</div>
            <h1>Benchmark Results</h1>
        </div>
        <div class="top-actions">
            <button class="btn" onclick="vscode.postMessage({type:'import'})">Import</button>
            <button class="btn primary" onclick="vscode.postMessage({type:'refresh'})">Refresh</button>
        </div>
    </div>

    ${totalFiles === 0 ? this.getEmptyHtml() : this.getTabsHtml(coding, grounding, search, engine, other)}

    <script nonce="${nonce}">
        const vscode = acquireVsCodeApi();
        function switchTab(name) {
            document.querySelectorAll('.tc').forEach(function(el) { el.classList.remove('active'); });
            document.querySelectorAll('.tab').forEach(function(el) { el.classList.remove('active'); });
            const c = document.getElementById('tab-' + name);
            if (c) c.classList.add('active');
            const tabs = document.querySelectorAll('.tab');
            for (const tab of tabs) {
                if (tab.dataset.tab === name) tab.classList.add('active');
            }
        }
        function openFile(path) {
            vscode.postMessage({type:'openFile', path: path});
        }
        function toggleDetail(id) {
            const el = document.getElementById(id);
            if (el) el.classList.toggle('visible');
        }
    </script>
</body>
</html>`;
    }

    private getEmptyHtml(): string {
        return `
        <div class="empty-state">
            <h2>No Benchmark Results</h2>
            <p>Run benchmarks with <code>python -m benchmark.coding.run</code> or click <strong>Import</strong> to load JSON results.</p>
            <p style="margin-top:12px">Results are stored in <code>benchmark/results/</code></p>
        </div>`;
    }

    private getTabsHtml(
        coding: BenchmarkFile[],
        grounding: BenchmarkFile[],
        search: BenchmarkFile[],
        engine: BenchmarkFile[],
        other: BenchmarkFile[],
    ): string {
        const tabs: { id: string; label: string; count: number; content: string }[] = [];
        tabs.push({ id: 'overview', label: 'Overview', count: 0, content: this.renderOverview(coding, grounding, search) });

        if (coding.length > 0) {
            tabs.push({ id: 'coding', label: 'Coding', count: coding.length, content: this.renderCodingTab(coding) });
        }
        if (grounding.length > 0) {
            tabs.push({ id: 'grounding', label: 'Grounding', count: grounding.length, content: this.renderGroundingTab(grounding) });
        }
        if (search.length > 0) {
            tabs.push({ id: 'search', label: 'Search', count: search.length, content: this.renderSearchTab(search) });
        }
        if (engine.length > 0 || other.length > 0) {
            tabs.push({ id: 'other', label: 'Other', count: engine.length + other.length, content: this.renderOtherTab([...engine, ...other]) });
        }

        const tabHeaders = tabs.map((t, i) =>
            `<div class="tab${i === 0 ? ' active' : ''}" data-tab="${t.id}" onclick="switchTab('${t.id}')">${t.label}${t.count ? ` (${t.count})` : ''}</div>`
        ).join('');

        const tabContents = tabs.map((t, i) =>
            `<div class="tc${i === 0 ? ' active' : ''}" id="tab-${t.id}">${t.content}</div>`
        ).join('');

        return `<div class="tabs">${tabHeaders}</div>${tabContents}`;
    }

    private renderOverview(coding: BenchmarkFile[], grounding: BenchmarkFile[], search: BenchmarkFile[]): string {
        const esc = escapeHtml;
        const latestCoding = coding[0];
        const latestGrounding = grounding[0];
        const latestSearch = search[0];

        let cardsHtml = '<div class="cards">';

        if (latestCoding) {
            const agg = latestCoding.data.summary ?? {};
            const synMean = agg.synapseed_mean ?? agg.single_synapseed?.mean;
            cardsHtml += this.renderCard(synMean !== undefined ? (synMean * 100).toFixed(0) + '%' : '--', 'Coding (SYN)', COLORS.INFO);
            const delta = agg.delta;
            if (delta !== undefined) {
                const cls = delta > 0 ? 'delta-pos' : delta < 0 ? 'delta-neg' : 'delta-zero';
                cardsHtml += `<div class="card"><div class="val ${cls}">${delta > 0 ? '+' : ''}${(delta * 100).toFixed(1)}%</div><div class="lbl">Coding Delta</div><div class="bar" style="width:${Math.abs(delta) * 100}%;background:${delta > 0 ? COLORS.PASS : COLORS.ERROR}"></div></div>`;
            }
        }

        if (latestGrounding) {
            const agg = latestGrounding.data.summary ?? {};
            const f1 = agg.grounded_f1?.f1;
            cardsHtml += this.renderCard(f1 !== undefined ? f1.toFixed(3) : '--', 'Grounding F1', COLORS.PASS);
        }

        if (latestSearch) {
            const agg = latestSearch.data.summary ?? {};
            cardsHtml += this.renderCard(agg.mrr !== undefined ? agg.mrr.toFixed(3) : '--', 'Search MRR', COLORS.PURPLE);
        }

        cardsHtml += this.renderCard(String(coding.length + grounding.length + search.length), 'Total Runs', COLORS.WARN);
        cardsHtml += '</div>';

        let tableHtml = `<div class="section"><h2><span class="dot" style="background:${COLORS.INFO}"></span> Latest Results</h2><table>`;
        tableHtml += '<thead><tr><th>Benchmark</th><th>Model</th><th>Key Metric</th><th>Timestamp</th><th>Modes</th></tr></thead><tbody>';

        for (const f of [latestCoding, latestGrounding, latestSearch].filter(Boolean)) {
            const d = f!.data;
            const meta = d.metadata ?? {};
            const bench = meta.benchmark_type ?? f!.type;
            const model = (d as Record<string, unknown>).model as string ?? '--';
            const ts = meta.timestamp ?? '--';
            const modes = (d as Record<string, unknown>).modes as string ?? '--';
            let metric = '--';

            if (f!.type === 'coding') {
                const syn = (d.summary as Record<string, unknown>)?.synapseed_mean as number | undefined;
                if (syn !== undefined) { metric = `SYN: ${(syn * 100).toFixed(1)}%`; }
            } else if (f!.type === 'grounding') {
                const gf1 = (d.summary as Record<string, unknown>)?.grounded_f1 as Record<string, number> | undefined;
                if (gf1?.f1 !== undefined) { metric = `F1: ${gf1.f1.toFixed(3)}`; }
            } else if (f!.type === 'search') {
                const mrr = (d.summary as Record<string, unknown>)?.mrr as number | undefined;
                if (mrr !== undefined) { metric = `MRR: ${mrr.toFixed(3)}`; }
            }

            const badge = `<span class="badge badge-${f!.type}">${esc(String(bench))}</span>`;
            tableHtml += `<tr class="click" onclick="openFile('${esc(f!.path)}')"><td>${badge}</td><td>${esc(String(model))}</td><td><strong>${esc(metric)}</strong></td><td>${esc(String(ts))}</td><td>${esc(String(modes))}</td></tr>`;
        }

        tableHtml += '</tbody></table></div>';
        return cardsHtml + tableHtml;
    }

    private renderCodingTab(files: BenchmarkFile[]): string {
        const esc = escapeHtml;
        let html = '';

        for (let fi = 0; fi < files.length; fi++) {
            const f = files[fi];
            const d = f.data as Record<string, unknown>;
            const meta = (d.metadata ?? {}) as Record<string, string>;
            const model = String(d.model ?? 'unknown');
            const modes = String(d.modes ?? 'baseline');
            const ts = meta.timestamp ?? '';
            const agg = (d.aggregate ?? d.summary ?? {}) as Record<string, unknown>;
            const conditions: string[] = (d.conditions as string[]) ?? [];
            const tasks: Record<string, unknown>[] = (d.tasks as Record<string, unknown>[]) ?? [];

            html += `<div class="section">`;
            html += `<h2><span class="dot" style="background:${COLORS.INFO}"></span> ${esc(model)} <span class="badge badge-coding">${esc(modes)}</span></h2>`;
            html += `<div style="font-size:11px;opacity:.5;margin-bottom:12px">${esc(f.filename)} &mdash; ${esc(ts)}</div>`;

            html += '<div class="cards">';
            for (const cKey of conditions) {
                const stats = agg[cKey] as Record<string, unknown> | undefined;
                if (!stats) { continue; }
                const mean = (stats.mean as number) ?? 0;
                const halluc = (stats.hallucinations as number) ?? 0;
                const color = mean > 0.7 ? COLORS.PASS : mean > 0.4 ? COLORS.WARN : COLORS.ERROR;
                html += `<div class="card"><div class="val" style="color:${color}">${(mean * 100).toFixed(1)}%</div><div class="lbl">${esc(String(stats.label ?? cKey))}</div><div class="bar" style="width:${mean * 100}%;background:${color}"></div></div>`;
                html += `<div class="card"><div class="val">${halluc}</div><div class="lbl">${esc(String(stats.label ?? cKey))} Halluc</div></div>`;
            }
            const delta = agg.delta as number | undefined;
            if (delta !== undefined) {
                const cls = delta > 0 ? 'delta-pos' : delta < 0 ? 'delta-neg' : 'delta-zero';
                html += `<div class="card"><div class="val ${cls}">${delta > 0 ? '+' : ''}${(delta * 100).toFixed(1)}%</div><div class="lbl">Delta</div></div>`;
            }
            html += '</div>';

            // Tasks table (simplified)
            if (tasks.length > 0) {
                html += `<div class="section"><h2><span class="dot" style="background:${COLORS.PASS}"></span> Tasks (${tasks.length})</h2>`;
                html += '<table><thead><tr><th>#</th><th>Task</th><th>Difficulty</th>';
                for (const cKey of conditions) {
                    const stats = agg[cKey] as Record<string, unknown> | undefined;
                    html += `<th>${esc(String(stats?.label ?? cKey))}</th>`;
                }
                html += '<th></th></tr></thead><tbody>';
                for (let ti = 0; ti < tasks.length; ti++) {
                    const t = tasks[ti];
                    const detailId = `detail-${fi}-${ti}`;
                    html += `<tr class="click" onclick="toggleDetail('${detailId}')">`;
                    html += `<td>${ti + 1}</td><td><strong>${esc(String(t.task ?? t.name ?? '--'))}</strong></td>`;
                    html += `<td>${esc(String(t.difficulty ?? '--'))}</td>`;
                    for (const cKey of conditions) {
                        const r = t[cKey] as Record<string, unknown> | undefined;
                        if (r && r.success !== false) {
                            const v = (r.composite as number) ?? 0;
                            const color = v > 0.7 ? COLORS.PASS : v > 0.4 ? COLORS.WARN : COLORS.ERROR;
                            html += `<td style="color:${color};font-weight:600">${(v * 100).toFixed(1)}%</td>`;
                        } else if (r && r.error) {
                            html += `<td style="color:${COLORS.ERROR}">ERR</td>`;
                        } else {
                            html += '<td>--</td>';
                        }
                    }
                    html += `<td><button class="expand-btn" onclick="event.stopPropagation();toggleDetail('${detailId}')">detail</button></td></tr>`;
                    html += `<tr><td colspan="${3 + conditions.length + 1}" style="padding:0;border:none;"><div class="task-detail" id="${detailId}">`;
                    for (const cKey of conditions) {
                        const r = t[cKey] as Record<string, unknown> | undefined;
                        if (!r) { continue; }
                        const stats = agg[cKey] as Record<string, unknown> | undefined;
                        html += `<div style="margin-bottom:8px"><strong>${esc(String(stats?.label ?? cKey))}</strong>`;
                        if (r.error) {
                            html += ` <span style="color:${COLORS.ERROR}">${esc(String(r.error))}</span>`;
                        } else {
                            const scores = ['composite', 'file_score', 'keyword_score', 'symbol_score', 'hallucinations', 'grounding_rate'];
                            html += '<div class="detail">';
                            for (const s of scores) {
                                if (r[s] !== undefined) {
                                    html += `<div class="row"><span>${esc(s)}</span><span>${typeof r[s] === 'number' ? (r[s] as number).toFixed(3) : r[s]}</span></div>`;
                                }
                            }
                            html += '</div>';
                        }
                        html += '</div>';
                    }
                    html += '</div></td></tr>';
                }
                html += '</tbody></table></div>';
            }

            html += '</div>';
            if (fi < files.length - 1) { html += '<hr style="border:none;border-top:1px solid var(--vscode-panel-border);margin:32px 0;">'; }
        }

        return html;
    }

    private renderGroundingTab(files: BenchmarkFile[]): string {
        const esc = escapeHtml;
        let html = '';
        for (const f of files) {
            const d = f.data as Record<string, unknown>;
            const model = String(d.model ?? 'unknown');
            const agg = (d.aggregate ?? d.summary ?? {}) as Record<string, unknown>;

            html += '<div class="section">';
            html += `<h2><span class="dot" style="background:${COLORS.PASS}"></span> ${esc(model)}</h2>`;
            html += `<div style="font-size:11px;opacity:.5;margin-bottom:12px">${esc(f.filename)}</div>`;
            html += '<div class="cards">';

            const gf1 = agg.grounded_f1 as Record<string, number> | undefined;
            const bf1 = agg.blind_f1 as Record<string, number> | undefined;
            html += this.renderCard(String((agg.grounded_total as number)?.toFixed(1) ?? '--'), 'Grounded Total', COLORS.PASS);
            html += this.renderCard(String((agg.blind_total as number)?.toFixed(1) ?? '--'), 'Blind Total', COLORS.ERROR);
            if (gf1) { html += this.renderCard(gf1.f1?.toFixed(3) ?? '--', 'Grounded F1', COLORS.PASS); }
            if (bf1) { html += this.renderCard(bf1.f1?.toFixed(3) ?? '--', 'Blind F1', COLORS.ERROR); }
            html += '</div>';

            if (bf1 && gf1) {
                const lift = (gf1.f1 ?? 0) - (bf1.f1 ?? 0);
                const cls = lift > 0 ? 'delta-pos' : lift < 0 ? 'delta-neg' : 'delta-zero';
                html += '<div class="detail">';
                html += `<div class="row"><span>F1 Lift</span><span class="${cls}">${lift > 0 ? '+' : ''}${lift.toFixed(3)}</span></div>`;
                html += `<div class="row"><span>Blind Precision</span><span>${(bf1.precision ?? 0).toFixed(3)}</span></div>`;
                html += `<div class="row"><span>Grounded Precision</span><span>${(gf1.precision ?? 0).toFixed(3)}</span></div>`;
                html += '</div>';
            }

            const results = (d.results ?? []) as Record<string, unknown>[];
            if (results.length > 0) {
                html += `<div class="section"><h2><span class="dot" style="background:${COLORS.INFO}"></span> Questions</h2>`;
                html += '<table><thead><tr><th>#</th><th>Question</th><th>Blind</th><th>Grounded</th><th>Max</th></tr></thead><tbody>';
                for (let i = 0; i < results.length; i++) {
                    const r = results[i];
                    html += `<tr><td>${i + 1}</td><td>${esc(String(r.question ?? '--'))}</td>`;
                    html += `<td>${r.blind_score ?? '--'}</td>`;
                    html += `<td>${r.grounded_score ?? '--'}</td>`;
                    html += `<td>${r.max_score ?? '--'}</td></tr>`;
                }
                html += '</tbody></table></div>';
            }

            html += '</div>';
        }
        return html;
    }

    private renderSearchTab(files: BenchmarkFile[]): string {
        const esc = escapeHtml;
        let html = '';
        for (const f of files) {
            const d = f.data as Record<string, unknown>;
            const agg = (d.aggregate ?? d.summary ?? {}) as Record<string, unknown>;
            const topK = (d.top_k as number) ?? 10;

            html += '<div class="section">';
            html += `<h2><span class="dot" style="background:${COLORS.PURPLE}"></span> Search Benchmark</h2>`;
            html += `<div style="font-size:11px;opacity:.5;margin-bottom:12px">${esc(f.filename)} &mdash; Top-K: ${topK}</div>`;
            html += '<div class="cards">';
            html += this.renderCard(((agg.mrr as number)?.toFixed(3)) ?? '--', 'MRR', COLORS.PURPLE);
            html += '</div>';

            const queries = (d.queries ?? []) as Record<string, unknown>[];
            if (queries.length > 0) {
                html += `<div class="section"><h2><span class="dot" style="background:${COLORS.INFO}"></span> Queries</h2>`;
                html += '<table><thead><tr><th>#</th><th>Query</th><th>MRR</th><th>P@K</th><th>R@K</th></tr></thead><tbody>';
                for (let i = 0; i < queries.length; i++) {
                    const q = queries[i];
                    const mrr = (q.mrr as number) ?? 0;
                    const mrrColor = mrr >= 0.5 ? COLORS.PASS : mrr >= 0.2 ? COLORS.WARN : COLORS.ERROR;
                    html += `<tr><td>${i + 1}</td><td>${esc(String(q.query ?? '--'))}</td>`;
                    html += `<td style="color:${mrrColor};font-weight:600">${mrr.toFixed(3)}</td>`;
                    html += `<td>${((q.precision as number) ?? 0).toFixed(3)}</td>`;
                    html += `<td>${((q.recall as number) ?? 0).toFixed(3)}</td>`;
                    html += '</tr>';
                }
                html += '</tbody></table></div>';
            }

            html += '</div>';
        }
        return html;
    }

    private renderOtherTab(files: BenchmarkFile[]): string {
        const esc = escapeHtml;
        let html = '<div class="section"><h2><span class="dot" style="background:' + COLORS.WARN + '"></span> Files</h2>';
        for (const f of files) {
            html += `<div class="file-item" onclick="openFile('${esc(f.path)}')">`;
            html += `<span class="fname">${esc(f.filename)}</span>`;
            html += `<span class="fmeta">${esc(f.type)}</span>`;
            html += '</div>';
        }
        html += '</div>';

        for (const f of files) {
            if (f.type === 'engine') {
                html += `<div class="section"><h2><span class="dot" style="background:${COLORS.WARN}"></span> Engine Report</h2>`;
                html += `<pre style="background:var(--vscode-textCodeBlock-background);padding:14px;border-radius:6px;font-size:11px;overflow-x:auto;white-space:pre-wrap">${esc(JSON.stringify(f.data, null, 2))}</pre>`;
                html += '</div>';
            }
        }
        return html;
    }

    private renderCard(value: string, label: string, color: string): string {
        const esc = escapeHtml;
        return `<div class="card"><div class="val" style="color:${color}">${esc(value)}</div><div class="lbl">${esc(label)}</div><div class="bar" style="width:100%;background:${color}"></div></div>`;
    }
}

type BenchType = 'coding' | 'grounding' | 'search' | 'engine' | 'unknown';

interface BenchmarkFile {
    filename: string;
    path: string;
    type: BenchType;
    data: BenchmarkFileData;
}
