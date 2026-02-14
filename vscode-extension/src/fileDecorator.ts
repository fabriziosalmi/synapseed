import * as vscode from 'vscode';
import { runSynapseedJson, getProjectRoot } from './cli';

interface AnalyzeResult {
    churn_score?: number;
    risk?: string;
}

/**
 * File decoration provider that shows risk badges on Explorer files.
 * High churn files get a flame badge, medium get a warning.
 */
export class SynapseedFileDecorator implements vscode.FileDecorationProvider {
    private _onDidChange = new vscode.EventEmitter<vscode.Uri | vscode.Uri[] | undefined>();
    readonly onDidChangeFileDecorations = this._onDidChange.event;

    private decorationCache = new Map<string, vscode.FileDecoration | null>();
    private analyzed = false;

    refresh(): void {
        this.decorationCache.clear();
        this.analyzed = false;
        this._onDidChange.fire(undefined);
    }

    async provideFileDecoration(uri: vscode.Uri): Promise<vscode.FileDecoration | undefined> {
        // Only decorate workspace files
        const root = getProjectRoot();
        if (!root || !uri.fsPath.startsWith(root)) { return; }

        // Only Rust/Python files
        if (!uri.fsPath.match(/\.(rs|py|ts|js)$/)) { return; }

        const relPath = uri.fsPath.replace(root + '/', '');
        if (this.decorationCache.has(relPath)) {
            return this.decorationCache.get(relPath) ?? undefined;
        }

        // Lazy analyze — don't block file explorer
        if (!this.analyzed) {
            this.analyzed = true;
            this.analyzeAll().catch(() => { /* best effort */ });
        }

        return undefined;
    }

    private async analyzeAll(): Promise<void> {
        try {
            // Use architect to get module-level risk data
            const result = await runSynapseedJson<any>(['architect'], { cache: true, cacheTtlMs: 120_000 });
            if (!result?.modules) { return; }

            for (const mod of result.modules) {
                if (mod.instability > 0.8) {
                    this.decorationCache.set(mod.name, new vscode.FileDecoration(
                        '🔥', `High instability: ${mod.instability.toFixed(2)}`,
                        new vscode.ThemeColor('charts.red'),
                    ));
                } else if (mod.instability > 0.5) {
                    this.decorationCache.set(mod.name, new vscode.FileDecoration(
                        '⚡', `Medium instability: ${mod.instability.toFixed(2)}`,
                        new vscode.ThemeColor('charts.yellow'),
                    ));
                }
            }

            this._onDidChange.fire(undefined);
        } catch {
            // Silent
        }
    }
}
