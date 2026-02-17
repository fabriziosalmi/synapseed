import * as vscode from 'vscode';
import { runSynapseed, runSynapseedJson, CliResult } from './cli';
import { log } from './log';
import { escapeHtml, gradeColor, metricColor, getNonce } from './html';
import { CACHE_TTL, TIMEOUT, COLORS, INTENT_COLORS, MAX_MODULES_DISPLAY } from './constants';
import { ArchitectReport, ArchitectModule, ArchitectRecommendation, TelemetryData, IntentCategory, DashboardMessage } from './types';

/**
 * Professional dashboard webview — tabs, metrics, charts.
 * v3: typed data, CSP nonce, no duplicate CLI calls, structured logging.
 */
export class DashboardPanel {
    private static instance: DashboardPanel | undefined;
    private readonly panel: vscode.WebviewPanel;

    static show(extensionUri: vscode.Uri): void {
        if (DashboardPanel.instance) {
            DashboardPanel.instance.panel.reveal(vscode.ViewColumn.One);
            DashboardPanel.instance.refresh();
            return;
        }
        new DashboardPanel(extensionUri);
    }

    private constructor(extensionUri: vscode.Uri) {
        this.panel = vscode.window.createWebviewPanel(
            'synapseed.dashboard',
            'SYNAPSEED Dashboard',
            vscode.ViewColumn.One,
            { enableScripts: true, retainContextWhenHidden: true },
        );
        this.panel.iconPath = new vscode.ThemeIcon('dashboard');
        DashboardPanel.instance = this;
        this.panel.onDidDispose(() => { DashboardPanel.instance = undefined; });

        this.panel.webview.onDidReceiveMessage(async (msg: DashboardMessage) => {
            try {
                if (msg.type === 'refresh') { await this.refresh(); }
                if (msg.type === 'openFile') {
                    await vscode.commands.executeCommand('vscode.open', vscode.Uri.file(msg.path));
                }
                if (msg.type === 'askAbout') {
                    await vscode.commands.executeCommand('synapseed.askQuestion');
                }
                if (msg.type === 'analyzeModule') {
                    const { AskPanel } = await import('./askPanel');
                    AskPanel.show(extensionUri, `analyze the module ${msg.name} — instability, coupling, and recommendations`);
                }
            } catch (err) {
                log.warn('Dashboard message handler error', err);
            }
        });

        this.refresh();
    }

    private async refresh(): Promise<void> {
        this.panel.webview.html = this.getLoadingHtml();

        const [statusRes, diagRes, archRes, intentRes, telRes] = await Promise.all([
            runSynapseed(['status'], { cache: true, cacheTtlMs: CACHE_TTL.STATUS }),
            runSynapseed(['diagnostics'], { cache: true, cacheTtlMs: CACHE_TTL.DIAGNOSTICS }),
            runSynapseedJson<ArchitectReport>(['architect'], { cache: true, cacheTtlMs: CACHE_TTL.ARCHITECTURE }),
            runSynapseed(['intent', '--limit', '10'], { cache: true }),
            runSynapseedJson<TelemetryData>(
                ['mcp', 'read', 'synapseed://telemetry/hotspots'],
                { timeoutMs: TIMEOUT.TELEMETRY, cache: true, cacheTtlMs: CACHE_TTL.TELEMETRY },
            ),
        ]);

        this.panel.webview.html = this.getHtml(statusRes, diagRes, archRes, intentRes, telRes);
    }

    private getLoadingHtml(): string {
        const nonce = getNonce();
        return `<!DOCTYPE html><html><head>
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'nonce-${nonce}';">
<style nonce="${nonce}">
    body { display:flex;align-items:center;justify-content:center;height:100vh;margin:0;
           font-family:var(--vscode-font-family);color:var(--vscode-foreground);background:var(--vscode-editor-background);}
    .loader{text-align:center;}
    .spinner{width:40px;height:40px;border:3px solid var(--vscode-panel-border);border-top-color:var(--vscode-focusBorder);
             border-radius:50%;animation:spin .8s linear infinite;margin:0 auto 16px;}
    @keyframes spin{to{transform:rotate(360deg);}}
</style></head><body><div class="loader"><div class="spinner"></div><p>Loading dashboard...</p></div></body></html>`;
    }

    private getHtml(
        statusRes: CliResult,
        diagRes: CliResult,
        archReport: ArchitectReport | null,
        intentRes: CliResult,
        telData: TelemetryData | null,
    ): string {
        const nonce = getNonce();
        const esc = escapeHtml;
        const isClean = diagRes.stdout?.startsWith('CLEAN');
        const grade = archReport?.grade ?? '?';
        const score = archReport?.score ?? 0;
        const modules = archReport?.module_count ?? 0;
        const edges = archReport?.edge_count ?? 0;
        const violations = archReport?.violations?.length ?? 0;
        const recommendations: ArchitectRecommendation[] = archReport?.recommendations ?? [];
        const avgInstability = archReport?.avg_instability?.toFixed(2) ?? '--';
        const maxCoupling = archReport?.max_coupling ?? 0;
        const density = archReport?.topological_density?.toFixed(4) ?? '--';
        const avgComplexity = archReport?.avg_complexity?.toFixed(1) ?? '--';

        // Parse intent categories
        const intentData: IntentCategory[] = [];
        for (const line of (intentRes.stdout ?? '').split('\n')) {
            const m = line.trim().match(/^(\w+):\s+(\d+)\s+commits?/i);
            if (m) { intentData.push({ cat: m[1], count: parseInt(m[2]) }); }
        }

        // All modules sorted by instability
        const allMods: ArchitectModule[] = (archReport?.modules ?? [])
            .sort((a: ArchitectModule, b: ArchitectModule) => b.instability - a.instability);
        const topMods = allMods.slice(0, MAX_MODULES_DISPLAY);

        // Security stats from status
        const secStats: Record<string, string> = {};
        if (statusRes.stdout) {
            const patterns: [string, RegExp][] = [
                ['DLP Scans', /DLP Scans:\s+(\d+)/],
                ['DLP Blocks', /DLP Blocks:\s+(\d+)/],
                ['Commands Allowed', /Commands Allowed:\s+(\d+)/],
                ['Commands Denied', /Commands Denied:\s+(\d+)/],
                ['Errors Prevented', /Errors Prevented:\s+(\d+)/],
            ];
            for (const [name, pat] of patterns) {
                const m = statusRes.stdout.match(pat);
                if (m) { secStats[name] = m[1]; }
            }
        }

        const gc = gradeColor(grade);

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
    .bar-row{display:flex;align-items:center;gap:8px;cursor:pointer;}
    .bar-row:hover{opacity:.85;}
    .bar-lbl{width:110px;font-size:11px;text-align:right;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;}
    .bar-track{flex:1;height:18px;background:var(--vscode-panel-border);border-radius:3px;overflow:hidden;position:relative;}
    .bar-fill{height:100%;border-radius:3px;transition:width .6s ease;}
    .bar-val{position:absolute;right:5px;top:1px;font-size:10px;font-weight:600;}
    .intent-grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(120px,1fr));gap:8px;}
    .intent-card{padding:10px;border-radius:6px;border:1px solid var(--vscode-panel-border);text-align:center;}
    .intent-card .cnt{font-size:22px;font-weight:700;}
    .intent-card .cat{font-size:10px;text-transform:uppercase;opacity:.5;}
    .rec-list{list-style:none;}
    .rec-list li{padding:8px 12px;border-left:3px solid var(--vscode-focusBorder);margin-bottom:6px;border-radius:0 4px 4px 0;background:var(--vscode-textBlockQuote-background);font-size:12px;}
    .detail{padding:12px;background:var(--vscode-textBlockQuote-background);border-radius:6px;margin:8px 0;font-size:12px;}
    .detail .row{display:flex;justify-content:space-between;padding:4px 0;border-bottom:1px solid var(--vscode-panel-border);}
    .detail .row:last-child{border-bottom:none;}
    .kbd{display:inline-block;padding:1px 5px;border-radius:3px;font-size:10px;font-family:monospace;background:var(--vscode-textCodeBlock-background);border:1px solid var(--vscode-panel-border);}
    .two-col{display:grid;grid-template-columns:1fr 1fr;gap:24px;}
    @media(max-width:600px){.two-col{grid-template-columns:1fr;}}
</style>
</head>
<body>
    <div class="top-bar">
        <div>
            <div class="label">SYNAPSEED</div>
            <h1>Project Dashboard</h1>
        </div>
        <div class="top-actions">
            <button class="btn" id="btn-ask">Ask</button>
            <button class="btn primary" id="btn-refresh">Refresh</button>
        </div>
    </div>

    <div class="tabs">
        <div class="tab active" data-tab="overview">Overview</div>
        <div class="tab" data-tab="modules">Modules (${modules})</div>
        <div class="tab" data-tab="security">Security</div>
        <div class="tab" data-tab="activity">Activity</div>
        <div class="tab" data-tab="raw">Raw</div>
    </div>

    <!-- Overview -->
    <div class="tc active" id="tab-overview">
        <div class="cards">
            <div class="card">
                <div class="val" style="color:${gc}">${esc(grade)}</div>
                <div class="lbl">Grade</div>
                <div class="bar" style="width:${score}%;background:${gc}"></div>
            </div>
            <div class="card">
                <div class="val">${score}</div>
                <div class="lbl">Health Score</div>
                <div class="bar" style="width:${score}%;background:${gc}"></div>
            </div>
            <div class="card">
                <div class="val" style="color:${isClean ? COLORS.PASS : COLORS.ERROR}">${isClean ? 'OK' : 'ERR'}</div>
                <div class="lbl">Build</div>
                <div class="bar" style="width:100%;background:${isClean ? COLORS.PASS : COLORS.ERROR}"></div>
            </div>
            <div class="card">
                <div class="val">${modules}</div>
                <div class="lbl">Modules</div>
                <div class="bar" style="width:100%;background:${COLORS.INFO}"></div>
            </div>
            <div class="card">
                <div class="val">${edges}</div>
                <div class="lbl">Dependencies</div>
                <div class="bar" style="width:100%;background:${COLORS.PURPLE}"></div>
            </div>
            <div class="card">
                <div class="val" style="color:${violations > 0 ? COLORS.ERROR : COLORS.PASS}">${violations}</div>
                <div class="lbl">Violations</div>
                <div class="bar" style="width:${violations > 0 ? '100' : '0'}%;background:${COLORS.ERROR}"></div>
            </div>
        </div>

        <div class="two-col">
            <div>
                ${topMods.length > 0 ? `
                <div class="section">
                    <h2><span class="dot" style="background:${COLORS.INFO}"></span> Instability (Top ${topMods.length})</h2>
                    <div class="bar-chart">
                        ${topMods.map((m: ArchitectModule) => {
            const pct = Math.round(m.instability * 100);
            const color = metricColor(pct);
            return `<div class="bar-row" data-module="${esc(m.module_name)}">
                            <div class="bar-lbl" title="${esc(m.module_name)}">${esc(m.module_name)}</div>
                            <div class="bar-track"><div class="bar-fill" style="width:${pct}%;background:${color}"></div><div class="bar-val">${pct}%</div></div>
                        </div>`;
        }).join('')}
                    </div>
                </div>` : ''}
            </div>
            <div>
                <div class="section">
                    <h2><span class="dot" style="background:${COLORS.PASS}"></span> Summary</h2>
                    <div class="detail">
                        <div class="row"><span>Avg Instability</span><span>${avgInstability}</span></div>
                        <div class="row"><span>Max Coupling</span><span>${maxCoupling}</span></div>
                        <div class="row"><span>Avg Complexity</span><span>${avgComplexity}</span></div>
                        <div class="row"><span>Density</span><span>${density}</span></div>
                    </div>
                </div>

                ${recommendations.length > 0 ? `
                <div class="section">
                    <h2><span class="dot" style="background:${COLORS.WARN}"></span> Recommendations</h2>
                    <ul class="rec-list">
                        ${recommendations.map((r: ArchitectRecommendation) => `<li><strong>P${r.priority}</strong> [${esc(r.category)}] ${esc(r.action)}</li>`).join('')}
                    </ul>
                </div>` : ''}
            </div>
        </div>
    </div>

    <!-- Modules -->
    <div class="tc" id="tab-modules">
        ${allMods.length > 0 ? `
        <div class="section">
            <h2><span class="dot" style="background:${COLORS.INFO}"></span> All Modules</h2>
            <table>
                <thead><tr><th>Module</th><th>Instability</th><th>Ce</th><th>Ca</th><th>Complexity</th><th></th></tr></thead>
                <tbody>
                    ${allMods.map((m: ArchitectModule) => {
            const pct = Math.round(m.instability * 100);
            const color = metricColor(pct);
            return `<tr class="click" data-module="${esc(m.module_name)}">
                        <td><strong>${esc(m.module_name)}</strong></td>
                        <td><span style="color:${color}">${m.instability.toFixed(2)}</span></td>
                        <td>${m.efferent_coupling}</td><td>${m.afferent_coupling}</td>
                        <td>${m.approx_complexity ?? '--'}</td>
                        <td style="opacity:.3;font-size:10px">analyze</td>
                    </tr>`;
        }).join('')}
                </tbody>
            </table>
        </div>` : '<p>No module data. Press <span class="kbd">Cmd+Alt+R</span> to refresh.</p>'}
    </div>

    <!-- Security -->
    <div class="tc" id="tab-security">
        <div class="two-col">
            <div class="section">
                <h2><span class="dot" style="background:${COLORS.PASS}"></span> Security Stats</h2>
                <div class="detail">
                    ${Object.entries(secStats).map(([k, v]) => {
            const isAlert = (k.includes('Block') || k.includes('Denied')) && parseInt(v) > 0;
            return `<div class="row"><span>${esc(k)}</span><span style="color:${isAlert ? COLORS.ERROR : 'inherit'};font-weight:600">${esc(v)}</span></div>`;
        }).join('') || '<div class="row"><span>No data</span><span>--</span></div>'}
                </div>
            </div>
            <div class="section">
                <h2><span class="dot" style="background:${COLORS.INFO}"></span> Telemetry</h2>
                ${telData ? `
                <div class="detail">
                    <div class="row"><span>Total Spans</span><span>${telData.total_spans ?? 0}</span></div>
                    <div class="row"><span>Unique Locations</span><span>${telData.unique_locations ?? 0}</span></div>
                    <div class="row"><span>Buffer Usage</span><span>${telData.buffer_usage ?? '0%'}</span></div>
                </div>
                ${(telData.hotspots ?? []).length > 0 ? `
                <table style="margin-top:12px">
                    <thead><tr><th>Hotspot</th><th>Calls</th><th>Avg (ms)</th><th>P95 (ms)</th></tr></thead>
                    <tbody>
                        ${(telData.hotspots ?? []).slice(0, 8).map((h) => {
            const sym = (h.key ?? '').split(':').pop() || h.key;
            return `<tr><td>${esc(sym)}</td><td>${h.call_count}</td><td>${h.avg_duration_ms?.toFixed(1) ?? '--'}</td><td>${h.p95_duration_ms?.toFixed(1) ?? '--'}</td></tr>`;
        }).join('')}
                    </tbody>
                </table>` : ''}
                ` : '<div class="detail"><div class="row"><span>No telemetry data</span><span>--</span></div></div>'}
            </div>
        </div>
    </div>

    <!-- Activity -->
    <div class="tc" id="tab-activity">
        ${intentData.length > 0 ? `
        <div class="section">
            <h2><span class="dot" style="background:${COLORS.PURPLE}"></span> Commit Intent</h2>
            <div class="intent-grid">
                ${intentData.map(d => {
            const color = INTENT_COLORS[d.cat] ?? '#888';
            return `<div class="intent-card"><div class="cnt" style="color:${color}">${d.count}</div><div class="cat">${esc(d.cat)}</div></div>`;
        }).join('')}
            </div>
        </div>` : '<p>No recent commit data.</p>'}

        <div class="section" style="margin-top:24px">
            <h2><span class="dot" style="background:${COLORS.PASS}"></span> Summary</h2>
            <div class="detail">
                <div class="row"><span>Architecture Grade</span><span style="color:${gc};font-weight:700">${esc(grade)}</span></div>
                <div class="row"><span>Health Score</span><span>${score}/100</span></div>
                <div class="row"><span>Build Status</span><span style="color:${isClean ? COLORS.PASS : COLORS.ERROR}">${isClean ? 'Clean' : 'Issues'}</span></div>
                <div class="row"><span>Total Modules</span><span>${modules}</span></div>
                <div class="row"><span>Total Dependencies</span><span>${edges}</span></div>
                <div class="row"><span>Avg Instability</span><span>${avgInstability}</span></div>
                <div class="row"><span>Max Coupling</span><span>${maxCoupling}</span></div>
                <div class="row"><span>Violations</span><span style="color:${violations > 0 ? COLORS.ERROR : COLORS.PASS}">${violations}</span></div>
            </div>
        </div>
    </div>

    <!-- Raw -->
    <div class="tc" id="tab-raw">
        <div class="section">
            <h2><span class="dot" style="background:${COLORS.PASS}"></span> Status</h2>
            <pre style="background:var(--vscode-textCodeBlock-background);padding:14px;border-radius:6px;font-size:11px;overflow-x:auto;white-space:pre-wrap">${esc(statusRes.stdout || 'N/A')}</pre>
        </div>
        ${archReport ? `
        <div class="section">
            <h2><span class="dot" style="background:${COLORS.INFO}"></span> Architecture</h2>
            <pre style="background:var(--vscode-textCodeBlock-background);padding:14px;border-radius:6px;font-size:11px;overflow-x:auto;white-space:pre-wrap">${esc(JSON.stringify(archReport, null, 2))}</pre>
        </div>` : ''}
    </div>

    <script nonce="${nonce}">
        const vscode = acquireVsCodeApi();
        function switchTab(name) {
            document.querySelectorAll('.tc').forEach(function(el) { el.classList.remove('active'); });
            document.querySelectorAll('.tab').forEach(function(el) { el.classList.remove('active'); });
            const c = document.getElementById('tab-' + name);
            if (c) c.classList.add('active');
            const tabs = document.querySelectorAll('.tab');
            const idx = {overview:0,modules:1,security:2,activity:3,raw:4};
            if (tabs[idx[name]]) tabs[idx[name]].classList.add('active');
        }
        // Top-bar buttons
        document.getElementById('btn-ask').addEventListener('click', function() { vscode.postMessage({type:'askAbout'}); });
        document.getElementById('btn-refresh').addEventListener('click', function() { vscode.postMessage({type:'refresh'}); });
        // Tab switching via delegation
        document.querySelector('.tabs').addEventListener('click', function(e) {
            var tab = e.target.closest('.tab');
            if (tab && tab.dataset.tab) switchTab(tab.dataset.tab);
        });
        // Module analysis via delegation
        document.addEventListener('click', function(e) {
            var el = e.target.closest('[data-module]');
            if (el) vscode.postMessage({type:'analyzeModule', name: el.dataset.module});
        });
        document.addEventListener('keydown', function(e) {
            if (e.key >= '1' && e.key <= '5' && !e.ctrlKey && !e.metaKey) {
                switchTab(['overview','modules','security','activity','raw'][parseInt(e.key)-1]);
            }
        });
    </script>
</body>
</html>`;
    }
}
