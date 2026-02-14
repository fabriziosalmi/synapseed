import * as vscode from 'vscode';
import { execFile } from 'child_process';
import { promisify } from 'util';

const execFileAsync = promisify(execFile);

/**
 * Get the configured synapseed binary path.
 */
export function getBinaryPath(): string {
    return vscode.workspace.getConfiguration('synapseed').get<string>('binaryPath', 'synapseed');
}

/**
 * Get the workspace root path.
 */
export function getProjectRoot(): string | undefined {
    const folders = vscode.workspace.workspaceFolders;
    if (!folders || folders.length === 0) {
        return undefined;
    }
    return folders[0].uri.fsPath;
}

export interface CliResult {
    stdout: string;
    stderr: string;
    success: boolean;
}

/**
 * Run a synapseed CLI command and return raw output.
 */
export async function runSynapseed(args: string[], timeoutMs: number = 30000): Promise<CliResult> {
    const bin = getBinaryPath();
    const root = getProjectRoot();
    if (!root) {
        return { stdout: '', stderr: 'No workspace folder open', success: false };
    }

    const fullArgs = [...args, '--project', root];

    try {
        const { stdout, stderr } = await execFileAsync(bin, fullArgs, {
            timeout: timeoutMs,
            maxBuffer: 10 * 1024 * 1024, // 10 MB
            env: { ...process.env, RUST_LOG: 'off' },
        });
        return { stdout: stdout.trim(), stderr: stderr.trim(), success: true };
    } catch (err: any) {
        // Some commands write to stdout even on non-zero exit
        const stdout = err.stdout?.toString().trim() ?? '';
        const stderr = err.stderr?.toString().trim() ?? err.message ?? 'Unknown error';
        return { stdout, stderr, success: false };
    }
}

/**
 * Run a synapseed CLI command that returns JSON.
 * Extracts JSON from mixed text/JSON output.
 */
export async function runSynapseedJson<T = any>(args: string[], timeoutMs?: number): Promise<T | null> {
    const result = await runSynapseed(args, timeoutMs);
    const text = result.stdout;
    if (!text) {
        return null;
    }

    // Try to parse the entire output as JSON first
    try {
        return JSON.parse(text);
    } catch {
        // Look for a JSON object or array in the output
        const jsonMatch = text.match(/(\{[\s\S]*\}|\[[\s\S]*\])/);
        if (jsonMatch) {
            try {
                return JSON.parse(jsonMatch[1]);
            } catch {
                return null;
            }
        }
        return null;
    }
}

/**
 * Parse key-value text output (like `status` and `diagnose` commands).
 * Returns a Map of section → entries.
 */
export function parseTextOutput(text: string): Map<string, Map<string, string>> {
    const sections = new Map<string, Map<string, string>>();
    let currentSection = 'main';
    sections.set(currentSection, new Map());

    for (const line of text.split('\n')) {
        const trimmed = line.trim();
        if (!trimmed || trimmed.startsWith('===')) {
            continue;
        }

        // Section header: --- Name ---
        const sectionMatch = trimmed.match(/^---\s+(.+?)\s+---$/);
        if (sectionMatch) {
            currentSection = sectionMatch[1];
            sections.set(currentSection, new Map());
            continue;
        }

        // Key: Value
        const kvMatch = trimmed.match(/^([^:]+?):\s+(.+)$/);
        if (kvMatch) {
            const section = sections.get(currentSection)!;
            section.set(kvMatch[1].trim(), kvMatch[2].trim());
            continue;
        }

        // Indented items (like plugin list or commit list)
        if (trimmed.startsWith('[') || trimmed.startsWith('-') || trimmed.match(/^[a-f0-9]{8}/)) {
            const section = sections.get(currentSection)!;
            const idx = section.size;
            section.set(`_item_${idx}`, trimmed);
        }
    }

    return sections;
}
