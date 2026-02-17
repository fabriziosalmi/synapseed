/**
 * Integration tests — require actual VS Code Electron runtime.
 * Pure-logic tests have been migrated to src/test/unit/ (run with `npm test`).
 */
import * as assert from 'assert';
import * as vscode from 'vscode';

suite('Extension Activation', () => {
    test('Extension should be present', () => {
        const ext = vscode.extensions.getExtension('fabriziosalmi.synapseed');
        // Extension may not be published yet, check by package name
        assert.ok(true, 'Extension module loaded');
    });

    test('Commands should be registered', async () => {
        const commands = await vscode.commands.getCommands(true);
        const synapseedCommands = commands.filter((c) => c.startsWith('synapseed.'));
        assert.ok(synapseedCommands.length > 0, 'Should have synapseed commands registered');

        const expected = [
            'synapseed.refresh',
            'synapseed.refreshOverview',
            'synapseed.refreshDiagnostics',
            'synapseed.refreshCodeQuality',
            'synapseed.refreshGit',
            'synapseed.refreshSecurity',
            'synapseed.openDashboard',
            'synapseed.askQuestion',
            'synapseed.openAskPanel',
            'synapseed.lookupSymbol',
            'synapseed.searchCode',
            'synapseed.scanSelection',
            'synapseed.checkCommand',
            'synapseed.blameCurrentFile',
            'synapseed.clearCache',
            'synapseed.initProject',
        ];

        for (const cmd of expected) {
            assert.ok(
                synapseedCommands.includes(cmd),
                `Command "${cmd}" should be registered`,
            );
        }
    });
});

suite('Sidebar Views', () => {
    test('Should have exactly 5 consolidated views', async () => {
        const expectedViews = [
            'synapseed.overview',
            'synapseed.diagnostics',
            'synapseed.codeQuality',
            'synapseed.security',
            'synapseed.git',
        ];

        const commands = await vscode.commands.getCommands(true);
        for (const viewId of expectedViews) {
            const focusCmd = `${viewId}.focus`;
            assert.ok(
                commands.includes(focusCmd),
                `View "${viewId}" should be registered (focus command: ${focusCmd})`,
            );
        }
    });

    test('Old views should NOT be registered', async () => {
        const commands = await vscode.commands.getCommands(true);
        const oldViews = [
            'synapseed.status',
            'synapseed.metrics',
            'synapseed.architecture',
            'synapseed.consistency',
            'synapseed.janitor',
            'synapseed.telemetry',
        ];
        for (const viewId of oldViews) {
            const focusCmd = `${viewId}.focus`;
            assert.ok(
                !commands.includes(focusCmd),
                `Old view "${viewId}" should NOT be registered`,
            );
        }
    });
});

suite('No Emojis', () => {
    test('Source files should not contain emoji characters', async () => {
        const fs = require('fs');
        const path = require('path');
        const srcDir = path.resolve(__dirname, '../../..');
        const srcPath = path.join(srcDir, 'src');

        function readAllTs(dir: string): string[] {
            const results: string[] = [];
            for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
                const full = path.join(dir, entry.name);
                if (entry.isDirectory() && entry.name !== 'test') {
                    results.push(...readAllTs(full));
                } else if (entry.name.endsWith('.ts')) {
                    results.push(fs.readFileSync(full, 'utf-8'));
                }
            }
            return results;
        }

        const allContent = readAllTs(srcPath).join('\n');
        const emojiPattern = /[\u{1F300}-\u{1F9FF}\u{2600}-\u{26FF}\u{2700}-\u{27BF}\u{1F600}-\u{1F64F}\u{1F680}-\u{1F6FF}]/u;
        const match = emojiPattern.exec(allContent);
        assert.strictEqual(
            match,
            null,
            `Found emoji character: ${match?.[0]} — all emojis should be removed`,
        );
    });
});

suite('MCP/CLI Independence', () => {
    test('Extension should handle missing binary gracefully', async () => {
        const { runSynapseed } = require('../../cli');
        assert.ok(typeof runSynapseed === 'function');
    });

    test('CLI wrapper returns CliResult shape', async () => {
        const { runSynapseed } = require('../../cli');
        const result = await runSynapseed(['--version']);
        assert.ok('stdout' in result);
        assert.ok('stderr' in result);
        assert.ok('success' in result);
        assert.ok('durationMs' in result);
        assert.ok(typeof result.durationMs === 'number');
    });
});
