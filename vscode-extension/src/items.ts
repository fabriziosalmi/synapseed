import * as vscode from 'vscode';

/**
 * Enhanced tree item with badges, commands, and context values.
 */
export class SynapseedItem extends vscode.TreeItem {
    children?: SynapseedItem[];

    constructor(
        public readonly label: string,
        opts: {
            description?: string;
            tooltip?: string | vscode.MarkdownString;
            icon?: string;
            state?: vscode.TreeItemCollapsibleState;
            children?: SynapseedItem[];
            contextValue?: string;
            command?: vscode.Command;
            resourceUri?: vscode.Uri;
            badge?: { value: number; tooltip: string };
        } = {},
    ) {
        super(label, opts.state ?? vscode.TreeItemCollapsibleState.None);
        if (opts.description !== undefined) { this.description = opts.description; }
        if (opts.tooltip) { this.tooltip = opts.tooltip; }
        if (opts.icon) { this.iconPath = new vscode.ThemeIcon(opts.icon); }
        if (opts.children) { this.children = opts.children; }
        if (opts.contextValue) { this.contextValue = opts.contextValue; }
        if (opts.command) { this.command = opts.command; }
        if (opts.resourceUri) { this.resourceUri = opts.resourceUri; }
    }
}

// ── Factory helpers ──────────────────────────────────────────────────

export function kvItem(key: string, value: string, icon?: string, tooltip?: string): SynapseedItem {
    return new SynapseedItem(key, {
        description: value,
        icon,
        tooltip: tooltip ?? `${key}: ${value}`,
    });
}

export function sectionItem(label: string, children: SynapseedItem[], icon?: string, collapsed = false): SynapseedItem {
    return new SynapseedItem(label, {
        description: `(${children.length})`,
        icon,
        state: children.length > 0
            ? (collapsed ? vscode.TreeItemCollapsibleState.Collapsed : vscode.TreeItemCollapsibleState.Expanded)
            : vscode.TreeItemCollapsibleState.None,
        children,
    });
}

export function clickableItem(label: string, description: string, command: string, args: any[], icon?: string): SynapseedItem {
    return new SynapseedItem(label, {
        description,
        icon,
        command: { command, title: label, arguments: args },
    });
}

export function fileItem(label: string, filePath: string, line?: number, icon?: string): SynapseedItem {
    const uri = vscode.Uri.file(filePath);
    const args: any[] = line
        ? [uri, { selection: new vscode.Range(line - 1, 0, line - 1, 0) }]
        : [uri];
    return new SynapseedItem(label, {
        description: filePath.split('/').pop(),
        icon: icon ?? 'go-to-file',
        command: { command: 'vscode.open', title: 'Open', arguments: args },
        resourceUri: uri,
        contextValue: 'synapseed.fileLink',
    });
}

export function statusItem(label: string, ok: boolean, goodText = 'OK', badText = 'Issues'): SynapseedItem {
    return kvItem(label, ok ? goodText : badText, ok ? 'pass' : 'error');
}

export function progressItem(label: string, percent: number, icon?: string): SynapseedItem {
    const bar = progressBar(percent);
    return new SynapseedItem(label, {
        description: `${bar} ${percent}%`,
        icon: icon ?? 'graph',
        tooltip: new vscode.MarkdownString(`**${label}**: ${percent}%`),
    });
}

export function loadingItem(text = 'Loading...'): SynapseedItem {
    return new SynapseedItem(text, { icon: 'loading~spin' });
}

export function errorItem(message: string): SynapseedItem {
    return new SynapseedItem('Error', { description: message, icon: 'error' });
}

export function emptyItem(message: string): SynapseedItem {
    return new SynapseedItem(message, { icon: 'info' });
}

export function separatorItem(): SynapseedItem {
    return new SynapseedItem('────────', { description: '' });
}

// ── Helpers ──────────────────────────────────────────────────────────

function progressBar(pct: number, len = 10): string {
    const filled = Math.round((pct / 100) * len);
    return '█'.repeat(filled) + '░'.repeat(len - filled);
}

export function gradeIcon(grade: string): string {
    switch (grade) {
        case 'A': return 'pass';
        case 'B': return 'info';
        case 'C': return 'warning';
        default: return 'error';
    }
}

export function severityIcon(severity: string): string {
    const s = severity.toLowerCase();
    if (s.includes('error')) { return 'error'; }
    if (s.includes('warn')) { return 'warning'; }
    return 'info';
}
