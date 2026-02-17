import * as vscode from 'vscode';
import { runSynapseedJson, getProjectRoot } from './cli';
import { log } from './log';
import { CACHE_TTL, INSTABILITY_THRESHOLDS, SUPPORTED_FILE_EXTENSIONS } from './constants';
import { ArchitectReport } from './types';

/**
 * File decoration provider that shows risk badges on Explorer files.
 * High instability files get a flame badge, medium get a warning.
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
        const root = getProjectRoot();
        if (!root || !uri.fsPath.startsWith(root)) { return; }

        if (!SUPPORTED_FILE_EXTENSIONS.test(uri.fsPath)) { return; }

        const relPath = uri.fsPath.replace(root + '/', '');
        if (this.decorationCache.has(relPath)) {
            return this.decorationCache.get(relPath) ?? undefined;
        }

        // Lazy analyze — don't block file explorer
        if (!this.analyzed) {
            this.analyzed = true;
            this.analyzeAll().catch((e: unknown) => log.warn('File decoration analysis failed', e));
        }

        return undefined;
    }

    private async analyzeAll(): Promise<void> {
        try {
            const result = await runSynapseedJson<ArchitectReport>(['architect'], { cache: true, cacheTtlMs: CACHE_TTL.ARCHITECTURE * 2 });
            if (!result?.modules) { return; }

            for (const mod of result.modules) {
                if (mod.instability > INSTABILITY_THRESHOLDS.HIGH) {
                    this.decorationCache.set(mod.module_name, new vscode.FileDecoration(
                        'H', `High instability: ${mod.instability.toFixed(2)}`,
                        new vscode.ThemeColor('charts.red'),
                    ));
                } else if (mod.instability > INSTABILITY_THRESHOLDS.MEDIUM) {
                    this.decorationCache.set(mod.module_name, new vscode.FileDecoration(
                        'M', `Medium instability: ${mod.instability.toFixed(2)}`,
                        new vscode.ThemeColor('charts.yellow'),
                    ));
                }
            }

            this._onDidChange.fire(undefined);
        } catch (e: unknown) {
            log.warn('analyzeAll failed', e);
        }
    }
}
