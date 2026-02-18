import * as vscode from 'vscode';
import { runSynapseedJson, getProjectRoot } from './cli';
import { globalCache } from './cache';
import { log } from './log';
import { CACHE_TTL, TIMEOUT } from './constants';
import { AnalyzeResult } from './types';

/**
 * CodeLens on Rust files showing SYNAPSEED analysis shortcuts.
 * - "Ask SYNAPSEED" on fn/struct/impl
 * - Churn risk on files with high churn
 */
export class SynapseedCodeLensProvider implements vscode.CodeLensProvider {
    private _onDidChange = new vscode.EventEmitter<void>();
    readonly onDidChangeCodeLenses = this._onDidChange.event;

    private analysisCache = new Map<string, AnalyzeResult>();

    refresh(): void {
        this.analysisCache.clear();
        globalCache.invalidate();
        this._onDidChange.fire();
    }

    async provideCodeLenses(
        document: vscode.TextDocument,
        _token: vscode.CancellationToken,
    ): Promise<vscode.CodeLens[]> {
        if (!vscode.workspace.getConfiguration('synapseed').get<boolean>('codeLens.enabled', true)) {
            return [];
        }

        const lenses: vscode.CodeLens[] = [];
        const text = document.getText();
        const root = getProjectRoot() ?? '';
        const relPath = document.uri.fsPath.replace(root + '/', '');

        // Add "Ask SYNAPSEED" lens on fn/struct/impl/enum/trait declarations
        const declPattern = /^(?:pub(?:\([^)]*\))?\s+)?(?:unsafe\s+)?(?:const\s+)?(?:async\s+)?(?:fn|struct|impl|enum|trait|mod)\s+(\w+)/gm;
        let match;
        while ((match = declPattern.exec(text)) !== null) {
            const line = document.positionAt(match.index).line;
            const range = new vscode.Range(line, 0, line, 0);

            // "Ask SYNAPSEED about this"
            lenses.push(new vscode.CodeLens(range, {
                title: '$(circuit-board) Ask SYNAPSEED',
                command: 'synapseed.askAboutSymbol',
                arguments: [match[1], relPath, line + 1],
                tooltip: `Ask SYNAPSEED about ${match[1]}`,
            }));

            // "Analyze history"
            lenses.push(new vscode.CodeLens(range, {
                title: '$(history) History',
                command: 'synapseed.analyzeFile',
                arguments: [relPath],
                tooltip: `Analyze git history for ${relPath}`,
            }));
        }

        // File-level lens for churn risk (first line)
        if (document.lineCount > 0) {
            const analysis = await this.getAnalysis(relPath);
            if (analysis && analysis.churn_score !== undefined) {
                const risk = analysis.risk ?? 'low';
                const churn = (analysis.churn_score * 100).toFixed(0);
                const icon = risk === 'high' ? '$(flame)' : risk === 'medium' ? '$(warning)' : '$(pass)';
                lenses.unshift(new vscode.CodeLens(new vscode.Range(0, 0, 0, 0), {
                    title: `${icon} Churn: ${churn}% · Risk: ${risk}`,
                    command: 'synapseed.analyzeFile',
                    arguments: [relPath],
                    tooltip: `Convergence: ${((analysis.convergence_rate ?? 0) * 100).toFixed(0)}% | Fix chains: ${analysis.fix_chain_count ?? 0}`,
                }));
            }
        }

        return lenses;
    }

    private async getAnalysis(relPath: string): Promise<AnalyzeResult | null> {
        if (this.analysisCache.has(relPath)) {
            return this.analysisCache.get(relPath) ?? null;
        }
        try {
            const result = await runSynapseedJson<AnalyzeResult>(
                ['analyze', relPath],
                { timeoutMs: TIMEOUT.TELEMETRY, cache: true, cacheTtlMs: CACHE_TTL.ARCHITECTURE },
            );
            if (result) { this.analysisCache.set(relPath, result); }
            return result;
        } catch (e: unknown) {
            log.warn(`CodeLens analysis failed for ${relPath}`, e);
            return null;
        }
    }
}
