import * as vscode from 'vscode';
import { runSynapseed, runSynapseedJson } from './cli';

/**
 * Full interactive dashboard webview with charts, live data, and clickable modules.
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

        this.panel.webview.onDidReceiveMessage(async (msg) => {
            if (msg.type === 'refresh') { await this.refresh(); }
            if (msg.type === 'openFile') {
                const uri = vscode.Uri.file(msg.path);
                await vscode.commands.executeCommand('vscode.open', uri);
            }
        });

        this.refresh();
    }

    private async refresh(): Promise<void> {
        this.panel.webview.html = this.getLoadingHtml();

        const [statusResult, diagResult, archResult, intentResult] = await Promise.all([
            runSynapseed(['status'], { cache: true }),
            runSynapseed(['diagnostics'], { cache: true }),
            runSynapseedJson<any>(['architect'], { cache: true }),
            runSynapseed(['intent', '--limit', '10'], { cache: true }),
        ]);

        this.panel.webview.html = this.getHtml(statusResult, diagResult, archResult, intentResult);
    }

    private getLoadingHtml(): string {
        return `<!DOCTYPE html><html><head><style>
            body { display: flex; align-items: center; justify-content: center; height: 100vh; margin: 0;
                   font-family: var(--vscode-font-family); color: var(--vscode-foreground); background: var(--vscode-editor-background); }
            .loader { text-align: center; }
            .spinner { width: 50px; height: 50px; border: 3px solid var(--vscode-panel-border); border-top-color: var(--vscode-focusBorder);
                       border-radius: 50%; animation: spin 0.8s linear infinite; margin: 0 auto 16px; }
            @keyframes spin { to { transform: rotate(360deg); } }
        </style></head><body><div class="loader"><div class="spinner"></div><p>Loading dashboard...</p></div></body></html>`;
    }

    private getHtml(statusResult: any, diagResult: any, archReport: any, intentResult: any): string {
        const isClean = diagResult.stdout?.startsWith('CLEAN');
        const grade = archReport?.grade ?? '?';
        const score = archReport?.score ?? 0;
        const modules = archReport?.module_count ?? 0;
        const edges = archReport?.edge_count ?? 0;
        const violations = archReport?.violations?.length ?? 0;
        const recommendations = archReport?.recommendations ?? [];
        const avgInstability = archReport?.avg_instability?.toFixed(2) ?? '—';
        const maxCoupling = archReport?.max_coupling ?? 0;

        // Parse intent categories
        const intentLines = (intentResult.stdout ?? '').split('\n').filter((l: string) => l.trim());
        const intentData: { cat: string; count: number }[] = [];
        for (const line of intentLines) {
            const m = line.match(/^(\w+):\s+(\d+)\s+commit/i);
            if (m) { intentData.push({ cat: m[1], count: parseInt(m[2]) }); }
        }

        // Parse modules for chart
        const mods = (archReport?.modules ?? [])
            .sort((a: any, b: any) => b.instability - a.instability)
            .slice(0, 12);

        const gradeColor = grade === 'A' ? '#4caf50' : grade === 'B' ? '#2196f3' : grade === 'C' ? '#ff9800' : '#f44336';

        return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
        font-family: var(--vscode-font-family); color: var(--vscode-foreground);
        background: var(--vscode-editor-background); padding: 24px; line-height: 1.5;
    }
    .top-bar {
        display: flex; align-items: center; justify-content: space-between;
        margin-bottom: 24px; padding-bottom: 16px;
        border-bottom: 2px solid var(--vscode-panel-border);
    }
    .top-bar h1 {
        font-size: 22px; font-weight: 700;
        background: linear-gradient(135deg, #4fc3f7, #ab47bc);
        -webkit-background-clip: text; -webkit-text-fill-color: transparent;
    }
    .refresh-btn {
        padding: 8px 16px; border: 1px solid var(--vscode-button-border, var(--vscode-panel-border));
        border-radius: 6px; background: var(--vscode-button-secondaryBackground);
        color: var(--vscode-button-secondaryForeground); cursor: pointer; font-size: 13px;
    }
    .refresh-btn:hover { opacity: 0.8; }
    .cards { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 16px; margin-bottom: 24px; }
    .card {
        border: 1px solid var(--vscode-panel-border); border-radius: 10px;
        padding: 20px; text-align: center; position: relative; overflow: hidden;
        background: var(--vscode-editor-background);
        transition: transform 0.15s, box-shadow 0.15s;
    }
    .card:hover { transform: translateY(-2px); box-shadow: 0 4px 12px rgba(0,0,0,0.2); }
    .card .value { font-size: 36px; font-weight: 800; line-height: 1.2; }
    .card .label { font-size: 12px; text-transform: uppercase; letter-spacing: 0.5px; opacity: 0.6; margin-top: 4px; }
    .card .bar { position: absolute; bottom: 0; left: 0; height: 3px; border-radius: 0 0 10px 10px; }
    .section { margin-bottom: 24px; }
    .section h2 { font-size: 16px; font-weight: 600; margin-bottom: 12px; display: flex; align-items: center; gap: 8px; }
    .section h2 .dot { width: 8px; height: 8px; border-radius: 50%; display: inline-block; }
    table { width: 100%; border-collapse: collapse; font-size: 13px; }
    th { text-align: left; padding: 8px 12px; border-bottom: 2px solid var(--vscode-panel-border); font-weight: 600; font-size: 11px; text-transform: uppercase; letter-spacing: 0.5px; opacity: 0.6; }
    td { padding: 8px 12px; border-bottom: 1px solid var(--vscode-panel-border); }
    tr:hover td { background: var(--vscode-list-hoverBackground); }
    .bar-chart { display: flex; flex-direction: column; gap: 8px; }
    .bar-row { display: flex; align-items: center; gap: 10px; }
    .bar-label { width: 120px; font-size: 12px; text-align: right; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .bar-track { flex: 1; height: 20px; background: var(--vscode-panel-border); border-radius: 4px; overflow: hidden; position: relative; }
    .bar-fill { height: 100%; border-radius: 4px; transition: width 0.6s ease; }
    .bar-value { position: absolute; right: 6px; top: 2px; font-size: 11px; font-weight: 600; }
    .intent-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr)); gap: 10px; }
    .intent-card { padding: 12px; border-radius: 8px; border: 1px solid var(--vscode-panel-border); text-align: center; }
    .intent-card .count { font-size: 24px; font-weight: 700; }
    .intent-card .cat { font-size: 11px; text-transform: uppercase; opacity: 0.6; }
    .rec-list { list-style: none; }
    .rec-list li { padding: 8px 12px; border-left: 3px solid var(--vscode-focusBorder); margin-bottom: 8px; border-radius: 0 6px 6px 0; background: var(--vscode-textBlockQuote-background); font-size: 13px; }
    .status-badge { display: inline-flex; align-items: center; gap: 4px; padding: 4px 10px; border-radius: 12px; font-size: 12px; font-weight: 600; }
    .badge-ok { background: rgba(76,175,80,0.15); color: #4caf50; }
    .badge-err { background: rgba(244,67,54,0.15); color: #f44336; }
</style>
</head>
<body>
    <div class="top-bar">
        <h1>⚡ SYNAPSEED Dashboard</h1>
        <button class="refresh-btn" onclick="vscode.postMessage({type:'refresh'})">↻ Refresh</button>
    </div>

    <div class="cards">
        <div class="card">
            <div class="value" style="color:${gradeColor}">${esc(grade)}</div>
            <div class="label">Architecture Grade</div>
            <div class="bar" style="width:${score}%; background:${gradeColor}"></div>
        </div>
        <div class="card">
            <div class="value">${score}</div>
            <div class="label">Health Score /100</div>
            <div class="bar" style="width:${score}%; background:${gradeColor}"></div>
        </div>
        <div class="card">
            <div class="value">${isClean ? '<span style="color:#4caf50">✓</span>' : '<span style="color:#f44336">✗</span>'}</div>
            <div class="label">Build Status</div>
            <div class="bar" style="width:100%; background:${isClean ? '#4caf50' : '#f44336'}"></div>
        </div>
        <div class="card">
            <div class="value">${modules}</div>
            <div class="label">Modules</div>
            <div class="bar" style="width:100%; background:#2196f3"></div>
        </div>
        <div class="card">
            <div class="value">${edges}</div>
            <div class="label">Dependencies</div>
            <div class="bar" style="width:100%; background:#9c27b0"></div>
        </div>
        <div class="card">
            <div class="value" style="color:${violations > 0 ? '#f44336' : '#4caf50'}">${violations}</div>
            <div class="label">Violations</div>
            <div class="bar" style="width:${violations > 0 ? '100' : '0'}%; background:#f44336"></div>
        </div>
    </div>

    ${mods.length > 0 ? `
    <div class="section">
        <h2><span class="dot" style="background:#2196f3"></span> Module Instability</h2>
        <div class="bar-chart">
            ${mods.map((m: any) => {
            const pct = Math.round(m.instability * 100);
            const color = pct > 80 ? '#f44336' : pct > 50 ? '#ff9800' : '#4caf50';
            return `<div class="bar-row">
                    <div class="bar-label" title="${esc(m.name)}">${esc(m.name)}</div>
                    <div class="bar-track">
                        <div class="bar-fill" style="width:${pct}%;background:${color}"></div>
                        <div class="bar-value">${pct}%</div>
                    </div>
                </div>`;
        }).join('')}
        </div>
    </div>` : ''}

    ${intentData.length > 0 ? `
    <div class="section">
        <h2><span class="dot" style="background:#ab47bc"></span> Recent Commit Intent</h2>
        <div class="intent-grid">
            ${intentData.map(d => {
            const colors: Record<string, string> = { fix: '#f44336', feature: '#4caf50', refactor: '#2196f3', security: '#ff9800', docs: '#9c27b0' };
            const c = colors[d.cat] ?? '#888';
            return `<div class="intent-card"><div class="count" style="color:${c}">${d.count}</div><div class="cat">${esc(d.cat)}</div></div>`;
        }).join('')}
        </div>
    </div>` : ''}

    ${recommendations.length > 0 ? `
    <div class="section">
        <h2><span class="dot" style="background:#ff9800"></span> Recommendations</h2>
        <ul class="rec-list">
            ${recommendations.map((r: string) => `<li>${esc(r)}</li>`).join('')}
        </ul>
    </div>` : ''}

    <div class="section">
        <h2><span class="dot" style="background:#4caf50"></span> Raw Status</h2>
        <pre style="background:var(--vscode-textCodeBlock-background);padding:16px;border-radius:8px;font-size:12px;overflow-x:auto">${esc(statusResult.stdout || 'N/A')}</pre>
    </div>

    <script>
        const vscode = acquireVsCodeApi();
        function esc(s) { return s; }
    </script>
</body>
</html>`;
    }
}

function esc(s: string): string {
    return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}
