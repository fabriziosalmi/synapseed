import * as vscode from 'vscode';
import { execFile } from 'child_process';
import { promisify } from 'util';
import { globalCache } from './cache';
import { log } from './log';
import { TIMEOUT, MAX_EXEC_BUFFER } from './constants';

const execFileAsync = promisify(execFile);

export function getBinaryPath(): string {
    return vscode.workspace.getConfiguration('synapseed').get<string>('binaryPath', 'synapseed');
}

export function getProjectRoot(): string | undefined {
    return vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
}

/** Returns project root or throws a descriptive error. */
export function getProjectRootOrThrow(): string {
    const root = getProjectRoot();
    if (!root) {
        throw new Error('No workspace folder open — SYNAPSEED requires an open project');
    }
    return root;
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
    const { timeoutMs = TIMEOUT.DEFAULT, cache = false, cacheTtlMs } = opts;
    const root = getProjectRoot();
    const cacheKey = `cli:${root ?? ''}:${args.join(' ')}`;

    if (cache) {
        const cached = globalCache.get<CliResult>(cacheKey);
        if (cached) { return cached; }
    }

    const bin = getBinaryPath();
    if (!root) {
        return { stdout: '', stderr: 'No workspace folder open', success: false, durationMs: 0 };
    }

    const fullArgs = [...args, '--project', root];
    const start = Date.now();

    try {
        const { stdout, stderr } = await execFileAsync(bin, fullArgs, {
            timeout: timeoutMs,
            maxBuffer: MAX_EXEC_BUFFER,
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
    } catch (err: unknown) {
        const execErr = err as { stdout?: Buffer | string; stderr?: Buffer | string; message?: string };
        const stdout = (execErr.stdout != null ? String(execErr.stdout).trim() : '');
        const stderr = (execErr.stderr != null ? String(execErr.stderr).trim() : '')
            || (execErr.message ?? 'Unknown error');
        log.warn(`CLI [${args[0]}] failed (${Date.now() - start}ms)`, err);
        return { stdout, stderr, success: false, durationMs: Date.now() - start };
    }
}

/**
 * Run a command that returns JSON. Callers must specify the expected type.
 */
export async function runSynapseedJson<T>(
    args: string[],
    opts?: { timeoutMs?: number; cache?: boolean; cacheTtlMs?: number },
): Promise<T | null> {
    const result = await runSynapseed(args, opts);
    const text = result.stdout;
    if (!text) { return null; }

    // Try parsing the full output as JSON first
    try { return JSON.parse(text) as T; } catch {
        log.warn(`CLI [${args[0]}] returned non-JSON output, attempting extraction`);
    }

    // Fallback: extract the first JSON object or array
    const m = text.match(/(\{[\s\S]*\}|\[[\s\S]*\])/);
    if (m) {
        try { return JSON.parse(m[1]) as T; } catch {
            log.warn(`CLI [${args[0]}] JSON extraction failed`);
        }
    }
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
        const sm = t.match(/^---\s+(.+?)\s+---\s*$/);
        if (sm) { cur = sm[1]; sections.set(cur, new Map()); continue; }
        const kv = t.match(/^([^:]+?):\s+(.+)$/);
        if (kv) {
            const section = sections.get(cur);
            if (section) { section.set(kv[1].trim(), kv[2].trim()); }
            continue;
        }
        if (t.startsWith('[') || t.startsWith('-') || /^[a-f0-9]{7,40}\b/.test(t)) {
            const s = sections.get(cur);
            if (s) { s.set(`_item_${s.size}`, t); }
        }
    }
    return sections;
}
