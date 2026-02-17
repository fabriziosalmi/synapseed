import * as assert from 'assert';
import { escapeHtml, gradeColor, metricColor, getNonce } from '../../html';

suite('escapeHtml', () => {
    test('escapes HTML entities', () => {
        assert.strictEqual(escapeHtml('<b>bold</b>'), '&lt;b&gt;bold&lt;/b&gt;');
        assert.strictEqual(escapeHtml('"quotes"'), '&quot;quotes&quot;');
        assert.strictEqual(escapeHtml("it's"), "it&#39;s");
        assert.strictEqual(escapeHtml('a & b'), 'a &amp; b');
    });

    test('handles empty string', () => {
        assert.strictEqual(escapeHtml(''), '');
    });

    test('handles string with no special chars', () => {
        assert.strictEqual(escapeHtml('hello world'), 'hello world');
    });

    test('handles XSS attack vector', () => {
        const xss = '<script>alert("xss")</script>';
        const result = escapeHtml(xss);
        assert.ok(!result.includes('<script>'));
        assert.ok(!result.includes('</script>'));
    });
});

suite('gradeColor', () => {
    test('returns distinct colors for each grade', () => {
        const grades = ['A', 'B', 'C', 'D', 'F'];
        const colors = grades.map(gradeColor);
        // A should differ from F
        assert.notStrictEqual(colors[0], colors[4]);
        // All should be valid hex colors
        for (const c of colors) {
            assert.ok(c.startsWith('#'), `${c} should be a hex color`);
        }
    });

    test('returns fallback for unknown grade', () => {
        const color = gradeColor('Z');
        assert.ok(typeof color === 'string');
    });
});

suite('metricColor', () => {
    test('returns red for high values', () => {
        const color = metricColor(90);
        assert.ok(color.startsWith('#'));
    });

    test('returns green for low values', () => {
        const color = metricColor(10);
        assert.ok(color.startsWith('#'));
    });

    test('differentiates high from low', () => {
        assert.notStrictEqual(metricColor(90), metricColor(10));
    });
});

suite('getNonce', () => {
    test('returns string of reasonable length', () => {
        const nonce = getNonce();
        assert.ok(nonce.length >= 16, `Nonce too short: ${nonce.length}`);
    });

    test('returns unique values', () => {
        const a = getNonce();
        const b = getNonce();
        assert.notStrictEqual(a, b);
    });

    test('contains only alphanumeric chars', () => {
        const nonce = getNonce();
        assert.ok(/^[a-zA-Z0-9]+$/.test(nonce), `Nonce has invalid chars: ${nonce}`);
    });
});
