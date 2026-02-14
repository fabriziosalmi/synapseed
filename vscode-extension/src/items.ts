import * as vscode from 'vscode';

/**
 * A tree item with a label and optional value, icon, and tooltip.
 */
export class SynapseedItem extends vscode.TreeItem {
    constructor(
        public readonly label: string,
        public readonly value?: string,
        public readonly collapsibleState: vscode.TreeItemCollapsibleState = vscode.TreeItemCollapsibleState.None,
        public readonly children?: SynapseedItem[],
        public readonly iconId?: string,
        public readonly contextValue?: string,
    ) {
        super(label, collapsibleState);

        if (value !== undefined) {
            this.description = value;
            this.tooltip = `${label}: ${value}`;
        }

        if (iconId) {
            this.iconPath = new vscode.ThemeIcon(iconId);
        }
    }
}

/**
 * Create a simple key-value item.
 */
export function kvItem(key: string, value: string, icon?: string): SynapseedItem {
    return new SynapseedItem(key, value, vscode.TreeItemCollapsibleState.None, undefined, icon);
}

/**
 * Create a section header with children.
 */
export function sectionItem(label: string, children: SynapseedItem[], icon?: string): SynapseedItem {
    return new SynapseedItem(
        label,
        `(${children.length})`,
        children.length > 0 ? vscode.TreeItemCollapsibleState.Expanded : vscode.TreeItemCollapsibleState.None,
        children,
        icon,
    );
}

/**
 * Create a loading/error placeholder.
 */
export function loadingItem(): SynapseedItem {
    return new SynapseedItem('Loading...', undefined, vscode.TreeItemCollapsibleState.None, undefined, 'loading~spin');
}

export function errorItem(message: string): SynapseedItem {
    return new SynapseedItem('Error', message, vscode.TreeItemCollapsibleState.None, undefined, 'error');
}

export function emptyItem(message: string): SynapseedItem {
    return new SynapseedItem(message, undefined, vscode.TreeItemCollapsibleState.None, undefined, 'info');
}
