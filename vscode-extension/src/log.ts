/** Structured logging via a dedicated Output Channel. */
import * as vscode from 'vscode';

let channel: vscode.OutputChannel | undefined;

function getChannel(): vscode.OutputChannel {
    if (!channel) {
        channel = vscode.window.createOutputChannel('SYNAPSEED');
    }
    return channel;
}

function timestamp(): string {
    return new Date().toISOString().slice(11, 23);
}

export const log = {
    info(msg: string): void {
        getChannel().appendLine(`[${timestamp()}] INFO  ${msg}`);
    },
    warn(msg: string, err?: unknown): void {
        const suffix = err instanceof Error ? `: ${err.message}` : err ? `: ${String(err)}` : '';
        getChannel().appendLine(`[${timestamp()}] WARN  ${msg}${suffix}`);
    },
    error(msg: string, err?: unknown): void {
        const suffix = err instanceof Error ? `: ${err.message}` : err ? `: ${String(err)}` : '';
        getChannel().appendLine(`[${timestamp()}] ERROR ${msg}${suffix}`);
    },
    show(): void {
        getChannel().show(true);
    },
    dispose(): void {
        channel?.dispose();
        channel = undefined;
    },
};
