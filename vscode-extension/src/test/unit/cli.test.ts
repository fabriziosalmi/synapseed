import * as assert from 'assert';
import { parseTextOutput, getBinaryPath } from '../../cli';

suite('parseTextOutput', () => {
    test('parses key-value lines into sections', () => {
        const text = [
            'Project: synapseed',
            'State: Healthy',
            '--- Metrics ---',
            'Files Indexed: 222',
            'Symbols: 1500',
            '--- Git ---',
            'Branch: main',
        ].join('\n');

        const sections = parseTextOutput(text);
        assert.ok(sections.has('main'));
        assert.ok(sections.has('Metrics'));
        assert.ok(sections.has('Git'));
        assert.strictEqual(sections.get('main')?.get('Project'), 'synapseed');
        assert.strictEqual(sections.get('main')?.get('State'), 'Healthy');
        assert.strictEqual(sections.get('Metrics')?.get('Files Indexed'), '222');
        assert.strictEqual(sections.get('Git')?.get('Branch'), 'main');
    });

    test('handles empty input', () => {
        const sections = parseTextOutput('');
        assert.ok(sections.has('main'));
        assert.strictEqual(sections.get('main')?.size, 0);
    });

    test('handles input with no sections', () => {
        const sections = parseTextOutput('Key: Value\nAnother: Thing');
        assert.strictEqual(sections.get('main')?.get('Key'), 'Value');
        assert.strictEqual(sections.get('main')?.get('Another'), 'Thing');
    });
});

suite('getBinaryPath', () => {
    test('returns a non-empty string', () => {
        const p = getBinaryPath();
        assert.ok(typeof p === 'string');
        assert.ok(p.length > 0);
    });
});
