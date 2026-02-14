import * as vscode from 'vscode';
import { runSynapseed } from '../cli';
import { SynapseedItem, kvItem, errorItem, emptyItem } from '../items';

/**
 * Security view — DLP scan status, command sentinel stats.
 */
export class SecurityProvider implements vscode.TreeDataProvider<SynapseedItem> {
    private _onDidChange = new vscode.EventEmitter<SynapseedItem | undefined>();
    readonly onDidChangeTreeData = this._onDidChange.event;

    private items: SynapseedItem[] = [emptyItem('Click refresh to load')];

    refresh(): void {
        this.loadData();
    }

    private async loadData(): Promise<void> {
        this.items = [new SynapseedItem('Loading...', undefined, vscode.TreeItemCollapsibleState.None, undefined, 'loading~spin')];
        this._onDidChange.fire(undefined);

        try {
            const items: SynapseedItem[] = [];

            // Run a test DLP scan to verify the system works
            const scanResult = await runSynapseed(['scan', '--content', 'test scan']);
            if (scanResult.stdout) {
                const isClean = scanResult.stdout.includes('CLEAN');
                items.push(kvItem('DLP Engine', isClean ? 'Active' : 'Alert', isClean ? 'pass' : 'warning'));
            }

            // Check command sentinel
            const checkResult = await runSynapseed(['check', 'cargo test']);
            if (checkResult.stdout) {
                const isAllowed = checkResult.stdout.includes('ALLOWED');
                items.push(kvItem('Command Sentinel', isAllowed ? 'Active' : 'Restricted', 'shield'));
            }

            // Get security stats from status
            const statusResult = await runSynapseed(['status']);
            if (statusResult.stdout) {
                const dlpScans = statusResult.stdout.match(/DLP Scans:\s+(\d+)/);
                const dlpBlocks = statusResult.stdout.match(/DLP Blocks:\s+(\d+)/);
                const cmdAllowed = statusResult.stdout.match(/Commands Allowed:\s+(\d+)/);
                const cmdDenied = statusResult.stdout.match(/Commands Denied:\s+(\d+)/);
                const errorsPrevent = statusResult.stdout.match(/Errors Prevented:\s+(\d+)/);

                if (dlpScans) { items.push(kvItem('DLP Scans', dlpScans[1], 'search')); }
                if (dlpBlocks) {
                    const blocks = parseInt(dlpBlocks[1]);
                    items.push(kvItem('DLP Blocks', dlpBlocks[1], blocks > 0 ? 'error' : 'pass'));
                }
                if (cmdAllowed) { items.push(kvItem('Commands Allowed', cmdAllowed[1], 'pass')); }
                if (cmdDenied) {
                    const denied = parseInt(cmdDenied[1]);
                    items.push(kvItem('Commands Denied', cmdDenied[1], denied > 0 ? 'warning' : 'pass'));
                }
                if (errorsPrevent) { items.push(kvItem('Errors Prevented', errorsPrevent[1], 'shield')); }
            }

            this.items = items.length > 0 ? items : [emptyItem('No security data')];
        } catch (e: any) {
            this.items = [errorItem(e.message)];
        }

        this._onDidChange.fire(undefined);
    }

    getTreeItem(element: SynapseedItem): vscode.TreeItem {
        return element;
    }

    getChildren(element?: SynapseedItem): SynapseedItem[] {
        if (element?.children) {
            return element.children;
        }
        return element ? [] : this.items;
    }
}
