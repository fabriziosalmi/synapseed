/**
 * Minimal vscode mock for unit tests that run without Electron.
 * Loaded via --require before any test that transitively imports vscode.
 */

/* eslint-disable @typescript-eslint/no-explicit-any */
const Module = require('module');
const path = require('path');

// The vscode stub — only what our modules actually use
const vscodeStub = {
    workspace: {
        getConfiguration: () => ({
            get: (_key: string, def: unknown) => def,
        }),
        workspaceFolders: undefined,
        isTrusted: true,
    },
    window: {
        createOutputChannel: () => ({
            appendLine: () => {},
            show: () => {},
            dispose: () => {},
        }),
        showErrorMessage: () => Promise.resolve(undefined),
        showWarningMessage: () => Promise.resolve(undefined),
        showInformationMessage: () => Promise.resolve(undefined),
    },
    TreeItem: class TreeItem {
        label: string;
        collapsibleState: number;
        description?: string;
        tooltip?: unknown;
        iconPath?: unknown;
        command?: unknown;
        contextValue?: string;
        resourceUri?: unknown;
        children?: unknown[];
        constructor(label: string, collapsibleState?: number) {
            this.label = label;
            this.collapsibleState = collapsibleState ?? 0;
        }
    },
    TreeItemCollapsibleState: { None: 0, Collapsed: 1, Expanded: 2 },
    ThemeIcon: class ThemeIcon {
        id: string;
        constructor(id: string) { this.id = id; }
    },
    Uri: {
        file: (p: string) => ({ fsPath: p, toString: () => `file://${p}` }),
        parse: (s: string) => ({ fsPath: s, toString: () => s }),
    },
    Range: class Range {
        start: { line: number; character: number };
        end: { line: number; character: number };
        constructor(sl: number, sc: number, el: number, ec: number) {
            this.start = { line: sl, character: sc };
            this.end = { line: el, character: ec };
        }
    },
    MarkdownString: class MarkdownString {
        value: string;
        constructor(value?: string) { this.value = value ?? ''; }
    },
    EventEmitter: class EventEmitter {
        event = () => {};
        fire() {}
        dispose() {}
    },
    DiagnosticSeverity: { Error: 0, Warning: 1, Information: 2, Hint: 3 },
    extensions: { getExtension: () => undefined },
    commands: { getCommands: () => Promise.resolve([]) },
    languages: {
        createDiagnosticCollection: () => ({
            set: () => {},
            clear: () => {},
            dispose: () => {},
        }),
    },
    ThemeColor: class ThemeColor {
        id: string;
        constructor(id: string) { this.id = id; }
    },
    Diagnostic: class Diagnostic {
        range: any;
        message: string;
        severity: number;
        source?: string;
        code?: string;
        constructor(range: any, message: string, severity?: number) {
            this.range = range;
            this.message = message;
            this.severity = severity ?? 0;
        }
    },
    FileDecoration: class FileDecoration {
        badge?: string;
        tooltip?: string;
        color?: any;
        constructor(badge?: string, tooltip?: string, color?: any) {
            this.badge = badge;
            this.tooltip = tooltip;
            this.color = color;
        }
    },
};

// Intercept require('vscode') — redirect to our stub
const originalResolveFilename = (Module as any)._resolveFilename;
(Module as any)._resolveFilename = function (request: string, parent: any, ...rest: any[]) {
    if (request === 'vscode') {
        // Return a sentinel path that we'll handle in the cache
        return '__vscode_mock__';
    }
    return originalResolveFilename.call(this, request, parent, ...rest);
};

// Register the stub in require.cache under the sentinel key
require.cache['__vscode_mock__'] = {
    id: '__vscode_mock__',
    filename: '__vscode_mock__',
    loaded: true,
    parent: null,
    children: [],
    paths: [],
    exports: vscodeStub,
    path: '',
    require: require,
    isPreloading: false,
} as any;
