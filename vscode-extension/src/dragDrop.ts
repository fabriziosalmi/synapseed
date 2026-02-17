import * as vscode from 'vscode';
import { log } from './log';
import { SynapseedItem } from './items';

/**
 * MIME type for SYNAPSEED tree items transferred via drag-and-drop.
 */
const SYNAPSEED_MIME = 'application/vnd.synapseed.item';

/**
 * Generic TreeDragAndDropController for all SYNAPSEED tree views.
 *
 * Supports:
 * - Dragging tree items (diagnostics, modules, proposals) as text to editors
 * - Dragging items into the Ask Panel (file URI drop triggers "ask about" query)
 * - Copy text representation of any tree item
 */
export class SynapseedDragDropController implements vscode.TreeDragAndDropController<SynapseedItem> {
    readonly dropMimeTypes: string[] = [SYNAPSEED_MIME, 'text/uri-list'];
    readonly dragMimeTypes: string[] = [SYNAPSEED_MIME, 'text/plain', 'text/uri-list'];

    handleDrag(
        source: readonly SynapseedItem[],
        dataTransfer: vscode.DataTransfer,
        _token: vscode.CancellationToken,
    ): void {
        if (source.length === 0) { return; }

        // Text representation — label + description
        const textParts = source.map(item => {
            const desc = typeof item.description === 'string' ? item.description : '';
            return desc ? `${item.label}: ${desc}` : `${item.label}`;
        });
        dataTransfer.set('text/plain', new vscode.DataTransferItem(textParts.join('\n')));

        // Structured data for internal use
        const payload = source.map(item => ({
            label: typeof item.label === 'string' ? item.label : '',
            description: typeof item.description === 'string' ? item.description : '',
            contextValue: item.contextValue ?? '',
            resourceUri: item.resourceUri?.toString() ?? '',
        }));
        dataTransfer.set(SYNAPSEED_MIME, new vscode.DataTransferItem(JSON.stringify(payload)));

        // If the item has a resource URI, also provide file URI for editor drops
        const uris = source
            .filter(item => item.resourceUri)
            .map(item => item.resourceUri!.toString());
        if (uris.length > 0) {
            dataTransfer.set('text/uri-list', new vscode.DataTransferItem(uris.join('\n')));
        }
    }

    handleDrop(
        _target: SynapseedItem | undefined,
        dataTransfer: vscode.DataTransfer,
        _token: vscode.CancellationToken,
    ): void | Thenable<void> {
        // Accept drops from explorer — open file in Ask panel
        const uriList = dataTransfer.get('text/uri-list');
        if (uriList) {
            const uris = uriList.value.split('\n').filter((u: string) => u.trim());
            for (const uriStr of uris) {
                try {
                    const uri = vscode.Uri.parse(uriStr);
                    vscode.commands.executeCommand('vscode.open', uri);
                } catch (e: unknown) { log.warn('Invalid URI in drop', e); }
            }
        }
    }
}

/**
 * Creates a drag-and-drop controller instance for use with tree views.
 */
export function createDragDropController(): SynapseedDragDropController {
    return new SynapseedDragDropController();
}
