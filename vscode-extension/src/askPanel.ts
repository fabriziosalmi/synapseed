import * as vscode from 'vscode';
import * as path from 'path';
import { runSynapseed, getProjectRoot } from './cli';
import { log } from './log';
import { escapeHtml, getNonce } from './html';
import { TIMEOUT, PANEL_READY_DELAY_MS } from './constants';
import { AskPanelMessage } from './types';

interface ConversationEntry {
    role: 'user' | 'assistant';
    text: string;
    durationMs?: number;
    timestamp: number;
}

/**
 * Webview panel for SYNAPSEED "Ask" — renders markdown responses.
 * v3: CSP nonce, typed messages, structured logging, improved regex.
 */
export class AskPanel {
    private static instance: AskPanel | undefined;
    private readonly panel: vscode.WebviewPanel;
    private readonly extensionUri: vscode.Uri;
    private conversation: ConversationEntry[] = [];
    private activeFileContext: string | undefined;
    private fileWatcher: vscode.Disposable | undefined;
    private initTimer: ReturnType<typeof setTimeout> | undefined;

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

    static showInColumn(extensionUri: vscode.Uri, column: vscode.ViewColumn, query?: string): AskPanel {
        if (AskPanel.instance) {
            AskPanel.instance.panel.reveal(column);
            if (query) { AskPanel.instance.ask(query); }
            return AskPanel.instance;
        }
        const p = new AskPanel(extensionUri, query, column);
        AskPanel.instance = p;
        return p;
    }

    static async exportConversation(): Promise<void> {
        if (!AskPanel.instance || AskPanel.instance.conversation.length === 0) {
            vscode.window.showWarningMessage('No conversation to export');
            return;
        }
        const lines = ['# SYNAPSEED Conversation', ''];
        for (const entry of AskPanel.instance.conversation) {
            const ts = new Date(entry.timestamp).toLocaleString();
            if (entry.role === 'user') {
                lines.push(`## You — ${ts}`, '', entry.text, '');
            } else {
                const dur = entry.durationMs ? ` (${entry.durationMs}ms)` : '';
                lines.push(`## SYNAPSEED${dur} — ${ts}`, '', entry.text, '');
            }
        }
        const uri = await vscode.window.showSaveDialog({
            defaultUri: vscode.Uri.file('synapseed-conversation.md'),
            filters: { Markdown: ['md'] },
        });
        if (uri) {
            await vscode.workspace.fs.writeFile(uri, Buffer.from(lines.join('\n'), 'utf-8'));
            vscode.window.showInformationMessage(`Conversation exported to ${uri.fsPath}`);
        }
    }

    static clearConversation(): void {
        if (!AskPanel.instance) { return; }
        AskPanel.instance.conversation = [];
        AskPanel.instance.panel.webview.postMessage({ type: 'clear' });
    }

    private constructor(extensionUri: vscode.Uri, initialQuery?: string, column?: vscode.ViewColumn) {
        this.extensionUri = extensionUri;
        this.panel = vscode.window.createWebviewPanel(
            'synapseed.ask',
            'SYNAPSEED Ask',
            column ?? vscode.ViewColumn.Beside,
            {
                enableScripts: true,
                retainContextWhenHidden: true,
                localResourceRoots: [vscode.Uri.joinPath(extensionUri, 'media')],
            },
        );
        this.panel.iconPath = new vscode.ThemeIcon('circuit-board');
        this.panel.webview.html = this.getHtml();

        // Track active file for context
        this.updateActiveFile();
        this.fileWatcher = vscode.window.onDidChangeActiveTextEditor(() => this.updateActiveFile());

        this.panel.webview.onDidReceiveMessage(async (msg: AskPanelMessage) => {
            switch (msg.type) {
                case 'ask':
                    await this.ask(msg.query);
                    break;
                case 'askAboutFile':
                    await this.ask(`analyze and explain ${msg.path}`);
                    break;
                case 'copy':
                    await vscode.env.clipboard.writeText(msg.text);
                    vscode.window.showInformationMessage('Copied to clipboard');
                    break;
                case 'export':
                    await AskPanel.exportConversation();
                    break;
                case 'clear':
                    AskPanel.clearConversation();
                    break;
                case 'openFile':
                    try {
                        const root = getProjectRoot() ?? '';
                        const fullPath = path.resolve(root, msg.path);
                        if (!fullPath.startsWith(root)) {
                            log.warn(`Path traversal blocked: ${msg.path}`);
                            break;
                        }
                        const uri = vscode.Uri.file(fullPath);
                        await vscode.commands.executeCommand('vscode.open', uri);
                    } catch (err) {
                        log.warn(`Failed to open file: ${msg.path}`, err);
                    }
                    break;
            }
        });

        this.panel.onDidDispose(() => {
            AskPanel.instance = undefined;
            this.fileWatcher?.dispose();
            if (this.initTimer) { clearTimeout(this.initTimer); }
        });

        if (initialQuery) {
            this.initTimer = setTimeout(async () => {
                this.initTimer = undefined;
                if (!AskPanel.instance) { return; }
                try { await this.ask(initialQuery); }
                catch (err) { log.error('Initial query failed', err); }
            }, PANEL_READY_DELAY_MS);
        }
    }

    private updateActiveFile(): void {
        const editor = vscode.window.activeTextEditor;
        if (!editor) { return; }
        const root = getProjectRoot() ?? '';
        const relPath = editor.document.uri.fsPath.replace(root + '/', '');
        if (relPath !== this.activeFileContext) {
            this.activeFileContext = relPath;
            this.panel.webview.postMessage({ type: 'activeFile', path: relPath });
        }
    }

    async ask(query: string): Promise<void> {
        this.conversation.push({ role: 'user', text: query, timestamp: Date.now() });
        this.panel.webview.postMessage({ type: 'thinking', query });

        const result = await runSynapseed(['ask', query, '--raw'], { timeoutMs: TIMEOUT.LONG });
        const response = result.success ? result.stdout : `Error: ${result.stderr}`;

        this.conversation.push({ role: 'assistant', text: response, durationMs: result.durationMs, timestamp: Date.now() });
        this.panel.webview.postMessage({
            type: 'response',
            query,
            response,
            durationMs: result.durationMs,
        });
    }

    private getHtml(): string {
        const nonce = getNonce();
        const esc = escapeHtml; // alias for use in template
        void esc; // used in embedded JS via the global function
        return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'nonce-${nonce}'; script-src 'nonce-${nonce}';">
<style nonce="${nonce}">
    * { box-sizing: border-box; }
    body {
        font-family: var(--vscode-font-family);
        color: var(--vscode-foreground);
        background: var(--vscode-editor-background);
        margin: 0; padding: 0;
        line-height: 1.6;
        display: flex; flex-direction: column; height: 100vh;
    }
    .header {
        display: flex; align-items: center; gap: 8px;
        border-bottom: 2px solid var(--vscode-focusBorder);
        padding: 12px 16px; flex-shrink: 0;
    }
    .header h1 {
        margin: 0; font-size: 18px; flex: 1;
        background: linear-gradient(135deg, #4fc3f7, #ab47bc);
        -webkit-background-clip: text; -webkit-text-fill-color: transparent;
    }
    .header-actions { display: flex; gap: 4px; }
    .header-btn {
        padding: 4px 8px; border: none; border-radius: 4px; cursor: pointer;
        background: var(--vscode-button-secondaryBackground);
        color: var(--vscode-button-secondaryForeground);
        font-size: 12px; opacity: 0.8; transition: opacity 0.15s;
    }
    .header-btn:hover { opacity: 1; }
    .context-bar {
        display: flex; align-items: center; gap: 8px;
        padding: 6px 16px; font-size: 12px;
        background: var(--vscode-textBlockQuote-background);
        border-bottom: 1px solid var(--vscode-panel-border);
        flex-shrink: 0;
    }
    .context-badge {
        display: inline-flex; align-items: center; gap: 4px;
        padding: 2px 8px; border-radius: 10px; font-size: 11px;
        background: var(--vscode-badge-background);
        color: var(--vscode-badge-foreground);
    }
    .input-area {
        padding: 12px 16px; flex-shrink: 0;
        border-bottom: 1px solid var(--vscode-panel-border);
    }
    .input-row { display: flex; gap: 8px; }
    #queryInput {
        flex: 1; padding: 10px 14px;
        background: var(--vscode-input-background);
        color: var(--vscode-input-foreground);
        border: 1px solid var(--vscode-input-border, #444);
        border-radius: 6px; font-size: 14px; outline: none;
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
        transition: opacity 0.15s; white-space: nowrap;
    }
    .send-btn:hover { opacity: 0.85; }
    .send-btn:disabled { opacity: 0.4; cursor: wait; }
    .drop-zone {
        margin: 8px 0 0; padding: 12px; text-align: center;
        border: 2px dashed var(--vscode-panel-border); border-radius: 8px;
        color: var(--vscode-descriptionForeground); font-size: 12px;
        transition: border-color 0.2s, background 0.2s;
        display: none;
    }
    .drop-zone.visible { display: block; }
    .drop-zone.active {
        border-color: var(--vscode-focusBorder);
        background: rgba(79,195,247,0.06);
    }
    .conversation { flex: 1; overflow-y: auto; padding: 16px; }
    .msg {
        margin-bottom: 16px; padding: 12px 16px;
        border-radius: 8px; animation: fadeIn 0.3s ease;
        position: relative;
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
    .msg.assistant .label {
        display: flex; align-items: center; justify-content: space-between;
        color: #ab47bc; font-weight: 600; font-size: 12px; margin-bottom: 4px;
    }
    .msg .content { white-space: pre-wrap; word-break: break-word; }
    .msg .meta { font-size: 11px; opacity: 0.5; margin-top: 6px; display: flex; gap: 12px; align-items: center; }
    .copy-btn {
        padding: 2px 8px; border: none; border-radius: 4px; cursor: pointer;
        background: var(--vscode-button-secondaryBackground);
        color: var(--vscode-button-secondaryForeground);
        font-size: 11px; opacity: 0; transition: opacity 0.15s;
    }
    .msg:hover .copy-btn { opacity: 0.7; }
    .copy-btn:hover { opacity: 1 !important; }
    .thinking {
        display: flex; align-items: center; gap: 8px;
        padding: 12px 16px; color: var(--vscode-descriptionForeground);
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
        position: relative;
    }
    pre .copy-code-btn {
        position: absolute; top: 4px; right: 4px;
        padding: 2px 6px; border: none; border-radius: 3px; cursor: pointer;
        background: var(--vscode-button-secondaryBackground);
        color: var(--vscode-button-secondaryForeground);
        font-size: 10px; opacity: 0; transition: opacity 0.15s;
    }
    pre:hover .copy-code-btn { opacity: 0.7; }
    pre .copy-code-btn:hover { opacity: 1; }
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
    h3 { margin: 16px 0 8px; font-size: 14px; font-weight: 600; }
    ul, ol { margin: 4px 0; padding-left: 20px; }
    li { margin: 2px 0; }
    blockquote {
        border-left: 3px solid var(--vscode-focusBorder);
        margin: 8px 0; padding: 4px 12px;
        background: var(--vscode-textBlockQuote-background);
    }
    table { width: 100%; border-collapse: collapse; margin: 8px 0; font-size: 13px; }
    th, td { padding: 6px 10px; border: 1px solid var(--vscode-panel-border); text-align: left; }
    th { background: var(--vscode-textBlockQuote-background); font-weight: 600; font-size: 12px; }
    .file-link {
        color: var(--vscode-textLink-foreground); cursor: pointer;
        text-decoration: underline; text-decoration-style: dotted;
    }
    .file-link:hover { text-decoration-style: solid; }
    @keyframes fadeIn { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: none; } }
    @keyframes blink { 0%, 80%, 100% { opacity: 0.2; } 40% { opacity: 1; } }
</style>
</head>
<body>
    <div class="header">
        <h1>SYNAPSEED Ask</h1>
        <div class="header-actions">
            <button class="header-btn" onclick="vscode.postMessage({type:'export'})" title="Export conversation">Export</button>
            <button class="header-btn" onclick="vscode.postMessage({type:'clear'})" title="Clear conversation">Clear</button>
        </div>
    </div>
    <div class="context-bar" id="contextBar" style="display:none">
        <span style="opacity:0.6">Context:</span>
        <span class="context-badge" id="activeFileBadge"></span>
    </div>
    <div class="input-area">
        <div class="input-row">
            <input id="queryInput" type="text" placeholder="Ask about your codebase... (drop files here)" autofocus />
            <button class="send-btn" id="sendBtn" onclick="send()">Ask</button>
        </div>
        <div class="drop-zone" id="dropZone">
            Drop a file to ask about it
        </div>
    </div>
    <div class="conversation" id="conversation">
        <div class="empty-state" id="emptyState">
            <div class="icon">&gt;_</div>
            <h2>Ask anything about your codebase</h2>
            <p>SYNAPSEED orchestrates AST analysis, search, git history, diagnostics, and security scanning in a single query.</p>
            <div class="hint-chips">
                <span class="hint-chip" onclick="askHint(this)">why is the build broken?</span>
                <span class="hint-chip" onclick="askHint(this)">explain the plugin system</span>
                <span class="hint-chip" onclick="askHint(this)">run a security audit</span>
                <span class="hint-chip" onclick="askHint(this)">what changed recently?</span>
                <span class="hint-chip" onclick="askHint(this)">show architecture health</span>
                <span class="hint-chip" onclick="askHint(this)">find security vulnerabilities</span>
            </div>
        </div>
    </div>
    <script nonce="${nonce}">
        const vscode = acquireVsCodeApi();
        const conv = document.getElementById('conversation');
        const input = document.getElementById('queryInput');
        const sendBtn = document.getElementById('sendBtn');
        const emptyState = document.getElementById('emptyState');
        const dropZone = document.getElementById('dropZone');
        const contextBar = document.getElementById('contextBar');
        const activeFileBadge = document.getElementById('activeFileBadge');
        let rawResponses = [];

        input.addEventListener('keydown', e => {
            if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(); }
            if (e.key === 'Escape') { input.blur(); }
        });
        document.addEventListener('keydown', e => {
            if ((e.ctrlKey || e.metaKey) && e.key === 'l') { e.preventDefault(); vscode.postMessage({type:'clear'}); }
            if (e.key === '/' && document.activeElement !== input) { e.preventDefault(); input.focus(); }
        });

        // File drop
        document.addEventListener('dragover', e => { e.preventDefault(); dropZone.classList.add('visible','active'); });
        document.addEventListener('dragleave', e => {
            if (!e.relatedTarget || e.relatedTarget === document.documentElement) {
                dropZone.classList.remove('visible','active');
            }
        });
        document.addEventListener('drop', e => {
            e.preventDefault();
            dropZone.classList.remove('visible','active');
            const text = e.dataTransfer.getData('text/plain');
            if (text) {
                input.value = 'explain ' + text;
                send();
            }
        });

        function askHint(el) { input.value = el.textContent; send(); }

        function send() {
            const q = input.value.trim();
            if (!q) return;
            vscode.postMessage({ type: 'ask', query: q });
            input.value = '';
        }

        function copyText(text) {
            vscode.postMessage({ type: 'copy', text: text });
        }

        window.addEventListener('message', e => {
            const msg = e.data;
            switch (msg.type) {
                case 'thinking':
                    if (emptyState) emptyState.remove();
                    sendBtn.disabled = true;
                    addUserMsg(msg.query);
                    addThinking();
                    break;
                case 'response':
                    removeThinking();
                    sendBtn.disabled = false;
                    rawResponses.push(msg.response);
                    addAssistantMsg(msg.response, msg.durationMs);
                    input.focus();
                    break;
                case 'activeFile':
                    contextBar.style.display = 'flex';
                    activeFileBadge.textContent = msg.path;
                    break;
                case 'clear':
                    conv.innerHTML = '';
                    rawResponses = [];
                    conv.appendChild(createEmptyState());
                    break;
            }
        });

        // Delegated click handler for file links (no inline onclick)
        conv.addEventListener('click', function(e) {
            const link = e.target.closest('.file-link');
            if (link && link.dataset.path) {
                vscode.postMessage({ type: 'openFile', path: link.dataset.path });
            }
        });

        function createEmptyState() {
            const el = document.createElement('div');
            el.className = 'empty-state';
            el.id = 'emptyState';
            el.innerHTML = '<div class="icon">&gt;_</div><h2>Ask anything about your codebase</h2>'
                + '<p>SYNAPSEED orchestrates AST analysis, search, git history, diagnostics, and security scanning.</p>';
            return el;
        }

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
            const el = document.getElementById('thinkingEl');
            if (el) el.remove();
        }

        function addAssistantMsg(text, ms) {
            const el = document.createElement('div');
            el.className = 'msg assistant';
            const labelHtml = '<div class="label"><span>SYNAPSEED</span>'
                + '<button class="copy-btn">Copy</button></div>';
            el.innerHTML = labelHtml + '<div class="content">' + formatResponse(text) + '</div>'
                + '<div class="meta"><span>' + (ms ? ms + 'ms' : '') + '</span></div>';
            el.querySelector('.copy-btn').addEventListener('click', function() {
                copyText(el.querySelector('.content').innerText);
            });
            conv.appendChild(el);
            el.querySelectorAll('pre').forEach(function(pre) {
                const btn = document.createElement('button');
                btn.className = 'copy-code-btn';
                btn.textContent = 'Copy';
                btn.addEventListener('click', function() { copyText(pre.innerText); });
                pre.appendChild(btn);
            });
            el.scrollIntoView({ behavior: 'smooth' });
        }

        function formatResponse(text) {
            text = esc(text);
            // Code blocks with language hint
            text = text.replace(/\`\`\`(\\w*)\\n?([\\s\\S]*?)\`\`\`/g, function(m, lang, code) {
                return '<pre data-lang="' + lang + '"><code>' + code + '</code></pre>';
            });
            // Inline code (no newlines allowed)
            text = text.replace(/\`([^\`\\n]+)\`/g, '<code>$1</code>');
            // Bold & italic
            text = text.replace(/\\*\\*(.+?)\\*\\*/g, '<strong>$1</strong>');
            text = text.replace(/\\*(.+?)\\*/g, '<em>$1</em>');
            // Headers
            text = text.replace(/^### (.+)$/gm, '<h3>$1</h3>');
            text = text.replace(/^## (.+)$/gm, '<h3 style="font-size:15px">$1</h3>');
            // Lists
            text = text.replace(/^- (.+)$/gm, '<li>$1</li>');
            text = text.replace(/(<li>.*<\\/li>\\n?)+/g, '<ul>$&</ul>');
            // Blockquotes
            text = text.replace(/^&gt; (.+)$/gm, '<blockquote>$1</blockquote>');
            // Markdown links — sanitize URL (only http/https allowed)
            text = text.replace(/\\[([^\\]]+)\\]\\(([^)]+)\\)/g, function(m, label, url) {
                if (/^https?:\\/\\//i.test(url)) {
                    return '<a href="' + url + '" rel="noopener">' + label + '</a>';
                }
                return label + ' (' + url + ')';
            });
            // File paths — make clickable via data attributes (no inline JS)
            text = text.replace(/((?:src|crates|bin|tests)\\/[\\w\\/.\\-]+\\.\\w+)(?::(\\d+))?/g, function(m, path, line) {
                const display = line ? path + ':' + line : path;
                return '<span class="file-link" data-path="' + path + '">' + display + '</span>';
            });
            return text;
        }

        function esc(s) {
            return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;').replace(/'/g,'&#39;');
        }
    </script>
</body>
</html>`;
    }
}
