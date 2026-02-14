import * as vscode from 'vscode';
import { runSynapseed } from './cli';

/**
 * Webview panel for SYNAPSEED "Ask" — renders markdown responses beautifully.
 */
export class AskPanel {
    private static instance: AskPanel | undefined;
    private readonly panel: vscode.WebviewPanel;
    private readonly extensionUri: vscode.Uri;

    static show(extensionUri: vscode.Uri, query?: string): AskPanel {
        if (AskPanel.instance) {
            AskPanel.instance.panel.reveal(vscode.ViewColumn.Beside);
            if (query) { AskPanel.instance.ask(query); }
            return AskPanel.instance;
        }
        const p = new AskPanel(extensionUri, query);
        AskPanel.instance = p;
        return p;
    }

    private constructor(extensionUri: vscode.Uri, initialQuery?: string) {
        this.extensionUri = extensionUri;
        this.panel = vscode.window.createWebviewPanel(
            'synapseed.ask',
            'SYNAPSEED Ask',
            vscode.ViewColumn.Beside,
            {
                enableScripts: true,
                retainContextWhenHidden: true,
                localResourceRoots: [vscode.Uri.joinPath(extensionUri, 'media')],
            },
        );
        this.panel.iconPath = new vscode.ThemeIcon('circuit-board');
        this.panel.webview.html = this.getHtml();

        this.panel.webview.onDidReceiveMessage(async (msg) => {
            if (msg.type === 'ask') {
                await this.ask(msg.query);
            }
        });

        this.panel.onDidDispose(() => { AskPanel.instance = undefined; });

        if (initialQuery) {
            setTimeout(() => this.ask(initialQuery), 300);
        }
    }

    async ask(query: string): Promise<void> {
        this.panel.webview.postMessage({ type: 'thinking', query });

        const result = await runSynapseed(['ask', query, '--raw'], { timeoutMs: 60_000 });
        const response = result.success ? result.stdout : `Error: ${result.stderr}`;

        this.panel.webview.postMessage({
            type: 'response',
            query,
            response,
            durationMs: result.durationMs,
        });
    }

    private getHtml(): string {
        return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<style>
    * { box-sizing: border-box; }
    body {
        font-family: var(--vscode-font-family);
        color: var(--vscode-foreground);
        background: var(--vscode-editor-background);
        margin: 0; padding: 16px;
        line-height: 1.6;
    }
    .header {
        display: flex; align-items: center; gap: 8px;
        border-bottom: 2px solid var(--vscode-focusBorder);
        padding-bottom: 12px; margin-bottom: 16px;
    }
    .header h1 {
        margin: 0; font-size: 18px;
        background: linear-gradient(135deg, #4fc3f7, #ab47bc);
        -webkit-background-clip: text; -webkit-text-fill-color: transparent;
    }
    .input-row {
        display: flex; gap: 8px; margin-bottom: 16px;
    }
    #queryInput {
        flex: 1; padding: 10px 14px;
        background: var(--vscode-input-background);
        color: var(--vscode-input-foreground);
        border: 1px solid var(--vscode-input-border, #444);
        border-radius: 6px; font-size: 14px;
        outline: none;
    }
    #queryInput:focus {
        border-color: var(--vscode-focusBorder);
        box-shadow: 0 0 0 1px var(--vscode-focusBorder);
    }
    #queryInput::placeholder { color: var(--vscode-input-placeholderForeground); }
    .send-btn {
        padding: 10px 20px; border: none; border-radius: 6px;
        background: var(--vscode-button-background);
        color: var(--vscode-button-foreground);
        font-size: 14px; cursor: pointer; font-weight: 600;
        transition: opacity 0.15s;
    }
    .send-btn:hover { opacity: 0.85; }
    .send-btn:disabled { opacity: 0.4; cursor: wait; }
    .conversation { max-width: 100%; }
    .msg {
        margin-bottom: 16px; padding: 12px 16px;
        border-radius: 8px; animation: fadeIn 0.3s ease;
    }
    .msg.user {
        background: var(--vscode-textBlockQuote-background);
        border-left: 3px solid var(--vscode-focusBorder);
    }
    .msg.user .label { color: var(--vscode-focusBorder); font-weight: 600; font-size: 12px; margin-bottom: 4px; }
    .msg.assistant {
        background: var(--vscode-editor-background);
        border: 1px solid var(--vscode-panel-border);
    }
    .msg.assistant .label { color: #ab47bc; font-weight: 600; font-size: 12px; margin-bottom: 4px; }
    .msg .content { white-space: pre-wrap; word-break: break-word; }
    .msg .meta { font-size: 11px; opacity: 0.5; margin-top: 6px; }
    .thinking {
        display: flex; align-items: center; gap: 8px;
        padding: 12px; color: var(--vscode-descriptionForeground);
    }
    .thinking .dots span {
        animation: blink 1.4s infinite both;
        font-size: 20px; line-height: 1;
    }
    .thinking .dots span:nth-child(2) { animation-delay: 0.2s; }
    .thinking .dots span:nth-child(3) { animation-delay: 0.4s; }
    pre {
        background: var(--vscode-textCodeBlock-background);
        padding: 12px; border-radius: 6px; overflow-x: auto;
        font-family: var(--vscode-editor-font-family); font-size: 13px;
    }
    code { font-family: var(--vscode-editor-font-family); font-size: 13px; }
    .empty-state {
        text-align: center; padding: 60px 20px;
        color: var(--vscode-descriptionForeground);
    }
    .empty-state .icon { font-size: 48px; margin-bottom: 16px; opacity: 0.3; }
    .empty-state h2 { margin: 0 0 8px; font-size: 16px; }
    .empty-state p { margin: 0; font-size: 13px; }
    .hint-chips {
        display: flex; flex-wrap: wrap; gap: 6px; justify-content: center;
        margin-top: 16px;
    }
    .hint-chip {
        padding: 6px 12px; border-radius: 16px; font-size: 12px;
        background: var(--vscode-badge-background);
        color: var(--vscode-badge-foreground);
        cursor: pointer; transition: opacity 0.15s;
    }
    .hint-chip:hover { opacity: 0.7; }
    @keyframes fadeIn { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: none; } }
    @keyframes blink { 0%, 80%, 100% { opacity: 0.2; } 40% { opacity: 1; } }
</style>
</head>
<body>
    <div class="header">
        <h1>⚡ SYNAPSEED Ask</h1>
    </div>
    <div class="input-row">
        <input id="queryInput" type="text" placeholder="Ask about your codebase..." autofocus />
        <button class="send-btn" id="sendBtn" onclick="send()">Ask</button>
    </div>
    <div class="conversation" id="conversation">
        <div class="empty-state" id="emptyState">
            <div class="icon">🧠</div>
            <h2>Ask anything about your codebase</h2>
            <p>SYNAPSEED orchestrates AST analysis, search, git history, diagnostics, and security scanning in a single query.</p>
            <div class="hint-chips">
                <span class="hint-chip" onclick="askHint(this)">why is the build broken?</span>
                <span class="hint-chip" onclick="askHint(this)">explain the plugin system</span>
                <span class="hint-chip" onclick="askHint(this)">run a security audit</span>
                <span class="hint-chip" onclick="askHint(this)">what changed recently?</span>
            </div>
        </div>
    </div>
    <script>
        const vscode = acquireVsCodeApi();
        const conv = document.getElementById('conversation');
        const input = document.getElementById('queryInput');
        const sendBtn = document.getElementById('sendBtn');
        const emptyState = document.getElementById('emptyState');

        input.addEventListener('keydown', e => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(); } });

        function askHint(el) { input.value = el.textContent; send(); }

        function send() {
            const q = input.value.trim();
            if (!q) return;
            vscode.postMessage({ type: 'ask', query: q });
            input.value = '';
        }

        window.addEventListener('message', e => {
            const msg = e.data;
            if (msg.type === 'thinking') {
                emptyState?.remove();
                sendBtn.disabled = true;
                addUserMsg(msg.query);
                addThinking();
            } else if (msg.type === 'response') {
                removeThinking();
                sendBtn.disabled = false;
                addAssistantMsg(msg.response, msg.durationMs);
                input.focus();
            }
        });

        function addUserMsg(q) {
            const el = document.createElement('div');
            el.className = 'msg user';
            el.innerHTML = '<div class="label">You</div><div class="content">' + esc(q) + '</div>';
            conv.appendChild(el);
            el.scrollIntoView({ behavior: 'smooth' });
        }

        function addThinking() {
            const el = document.createElement('div');
            el.id = 'thinkingEl';
            el.className = 'thinking';
            el.innerHTML = 'Analyzing <span class="dots"><span>.</span><span>.</span><span>.</span></span>';
            conv.appendChild(el);
            el.scrollIntoView({ behavior: 'smooth' });
        }

        function removeThinking() {
            document.getElementById('thinkingEl')?.remove();
        }

        function addAssistantMsg(text, ms) {
            const el = document.createElement('div');
            el.className = 'msg assistant';
            el.innerHTML = '<div class="label">SYNAPSEED</div><div class="content">' + formatResponse(text) + '</div>'
                + '<div class="meta">' + (ms ? ms + 'ms' : '') + '</div>';
            conv.appendChild(el);
            el.scrollIntoView({ behavior: 'smooth' });
        }

        function formatResponse(text) {
            // Basic code block formatting
            text = esc(text);
            text = text.replace(/\`\`\`(\\w*)?\\n([\\s\\S]*?)\`\`\`/g, '<pre><code>$2</code></pre>');
            text = text.replace(/\`([^\`]+)\`/g, '<code>$1</code>');
            text = text.replace(/\\*\\*(.+?)\\*\\*/g, '<strong>$1</strong>');
            return text;
        }

        function esc(s) { return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;'); }
    </script>
</body>
</html>`;
    }
}
