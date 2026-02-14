import * as vscode from 'vscode';
import { runSynapseed, runSynapseedJson } from './cli';

/**
 * Bridges SYNAPSEED diagnostics into VS Code's native DiagnosticCollection.
 * Errors appear in the Problems panel with squiggly underlines.
 */
export class DiagnosticBridge implements vscode.Disposable {
    private readonly collection: vscode.DiagnosticCollection;
    private disposed = false;

    constructor() {
        this.collection = vscode.languages.createDiagnosticCollection('synapseed');
    }

    async refresh(): Promise<void> {
        if (this.disposed) { return; }

        try {
            const result = await runSynapseed(['diagnostics']);
            if (!result.stdout) { return; }

            const text = result.stdout;
            if (text.startsWith('CLEAN')) {
                this.collection.clear();
                return;
            }

            const diagnosticMap = new Map<string, vscode.Diagnostic[]>();
            const root = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? '';

            // Parse error/warning lines
            // Formats:
            //   error[E0425]: cannot find value `x` in this scope --> src/main.rs:10:5
            //   warning: unused variable `y` --> src/lib.rs:20:1
            const pattern = /^(error|warning)(?:\[([A-Z]\d+)\])?:\s*(.+?)(?:\s+-->\s+(.+?):(\d+):(\d+))?$/gm;
            let m;
            while ((m = pattern.exec(text)) !== null) {
                const severity = m[1] === 'error'
                    ? vscode.DiagnosticSeverity.Error
                    : vscode.DiagnosticSeverity.Warning;
                const code = m[2] || undefined;
                const message = m[3].trim();
                const file = m[4] ?? '';
                const line = parseInt(m[5] ?? '1') - 1;
                const col = parseInt(m[6] ?? '1') - 1;

                if (!file) { continue; }

                const fullPath = file.startsWith('/') ? file : `${root}/${file}`;
                const range = new vscode.Range(line, col, line, col + 20);
                const diag = new vscode.Diagnostic(range, message, severity);
                diag.source = 'synapseed';
                if (code) { diag.code = code; }

                const existing = diagnosticMap.get(fullPath) ?? [];
                existing.push(diag);
                diagnosticMap.set(fullPath, existing);
            }

            this.collection.clear();
            for (const [path, diags] of diagnosticMap) {
                this.collection.set(vscode.Uri.file(path), diags);
            }
        } catch {
            // Silently fail — diagnostics are best-effort
        }
    }

    dispose(): void {
        this.disposed = true;
        this.collection.dispose();
    }
}
