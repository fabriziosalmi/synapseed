import * as vscode from 'vscode';
import { execFile } from 'child_process';
import { promisify } from 'util';
import { globalCache } from './cache';

const execFileAsync = promisify(execFile);

export function getBinaryPath(): string {
    return vscode.workspace.getConfiguration('synapseed').get<string>('binaryPath', 'synapseed');
}

export function getProjectRoot(): string | undefined {
    return vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
}

export interface CliResult {
    stdout: string;
    stderr: string;
    success: boolean;
    durationMs: number;
}

/**
 * Run a synapseed CLI command. Supports caching, timeout, and progress indication.
 */
export async function runSynapseed(
    args: string[],
    opts: { timeoutMs?: number; cache?: boolean; cacheTtlMs?: number } = {},
): Promise<CliResult> {
    const { timeoutMs = 30_000, cache = false, cacheTtlMs } = opts;
    const cacheKey = `cli:${args.join(' ')}`;

    if (cache) {
        const cached = globalCache.get<CliResult>(cacheKey);
        if (cached) { return cached; }
    }

    const bin = getBinaryPath();
    const root = getProjectRoot();
    if (!root) {
        return { stdout: '', stderr: 'No workspace folder open', success: false, durationMs: 0 };
    }

    const fullArgs = [...args, '--project', root];
    const start = Date.now();

    try {
        const { stdout, stderr } = await execFileAsync(bin, fullArgs, {
            timeout: timeoutMs,
            maxBuffer: 10 * 1024 * 1024,
            env: { ...process.env, RUST_LOG: 'off' },
        });
        const result: CliResult = {
            stdout: stdout.trim(),
            stderr: stderr.trim(),
            success: true,
            durationMs: Date.now() - start,
        };
        if (cache) { globalCache.set(cacheKey, result, cacheTtlMs); }
        return result;
    } catch (err: any) {
        const stdout = err.stdout?.toString().trim() ?? '';
        const stderr = err.stderr?.toString().trim() ?? err.message ?? 'Unknown error';
        return { stdout, stderr, success: false, durationMs: Date.now() - start };
    }
}

/**
 * Run a command that returns JSON.
 */
export async function runSynapseedJson<T = any>(
    args: string[],
    opts?: { timeoutMs?: number; cache?: boolean; cacheTtlMs?: number },
): Promise<T | null> {
    const result = await runSynapseed(args, opts);
    const text = result.stdout;
    if (!text) { return null; }
    try { return JSON.parse(text); } catch { /* fallthrough */ }
    const m = text.match(/(\{[\s\S]*\}|\[[\s\S]*\])/);
    if (m) { try { return JSON.parse(m[1]); } catch { /* ignore */ } }
    return null;
}

/**
 * Parse key-value text output. Returns Map<section, Map<key, value>>.
 */
export function parseTextOutput(text: string): Map<string, Map<string, string>> {
    const sections = new Map<string, Map<string, string>>();
    let cur = 'main';
    sections.set(cur, new Map());

    for (const line of text.split('\n')) {
        const t = line.trim();
        if (!t || t.startsWith('===')) { continue; }
        const sm = t.match(/^---\s+(.+?)\s+---$/);
        if (sm) { cur = sm[1]; sections.set(cur, new Map()); continue; }
        const kv = t.match(/^([^:]+?):\s+(.+)$/);
        if (kv) { sections.get(cur)!.set(kv[1].trim(), kv[2].trim()); continue; }
        if (t.startsWith('[') || t.startsWith('-') || t.match(/^[a-f0-9]{8}/)) {
            const s = sections.get(cur)!;
            s.set(`_item_${s.size}`, t);
        }
    }
    return sections;
}
