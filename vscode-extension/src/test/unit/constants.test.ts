import * as assert from 'assert';
import {
    CACHE_TTL, TIMEOUT, MAX_EXEC_BUFFER, MAX_CACHE_ENTRIES,
    INSTABILITY_THRESHOLDS, COLORS, GRADE_COLORS, INTENT_COLORS,
    SUPPORTED_LANGUAGES, SUPPORTED_FILE_EXTENSIONS,
    STATUS_BAR_PRIORITY, SAVE_DEBOUNCE_MS,
} from '../../constants';

suite('Constants integrity', () => {
    test('CACHE_TTL values are positive numbers', () => {
        for (const [key, val] of Object.entries(CACHE_TTL)) {
            assert.ok(typeof val === 'number' && val > 0, `CACHE_TTL.${key} = ${val}`);
        }
    });

    test('TIMEOUT values are positive and >= CACHE_TTL', () => {
        assert.ok(TIMEOUT.DEFAULT > 0);
        assert.ok(TIMEOUT.LONG > TIMEOUT.DEFAULT);
    });

    test('MAX_EXEC_BUFFER is reasonable (1-50 MB)', () => {
        assert.ok(MAX_EXEC_BUFFER >= 1024 * 1024);
        assert.ok(MAX_EXEC_BUFFER <= 50 * 1024 * 1024);
    });

    test('MAX_CACHE_ENTRIES is positive', () => {
        assert.ok(MAX_CACHE_ENTRIES > 0);
    });

    test('INSTABILITY_THRESHOLDS are ordered', () => {
        assert.ok(INSTABILITY_THRESHOLDS.HIGH > INSTABILITY_THRESHOLDS.MEDIUM);
    });

    test('COLORS are valid hex', () => {
        for (const [key, val] of Object.entries(COLORS)) {
            assert.ok(/^#[0-9a-fA-F]{6}$/.test(val), `COLORS.${key} = ${val}`);
        }
    });

    test('GRADE_COLORS covers A-F', () => {
        for (const grade of ['A', 'B', 'C', 'D', 'F']) {
            assert.ok(GRADE_COLORS[grade], `Missing GRADE_COLORS.${grade}`);
        }
    });

    test('INTENT_COLORS covers categories', () => {
        for (const cat of ['fix', 'feature', 'refactor', 'security', 'docs']) {
            assert.ok(INTENT_COLORS[cat], `Missing INTENT_COLORS.${cat}`);
        }
    });

    test('SUPPORTED_LANGUAGES is non-empty', () => {
        assert.ok(SUPPORTED_LANGUAGES.length >= 2);
    });

    test('SUPPORTED_FILE_EXTENSIONS regex works', () => {
        assert.ok(SUPPORTED_FILE_EXTENSIONS.test('file.rs'));
        assert.ok(SUPPORTED_FILE_EXTENSIONS.test('file.py'));
        assert.ok(SUPPORTED_FILE_EXTENSIONS.test('file.ts'));
        assert.ok(!SUPPORTED_FILE_EXTENSIONS.test('file.txt'));
        assert.ok(!SUPPORTED_FILE_EXTENSIONS.test('file.md'));
    });

    test('STATUS_BAR_PRIORITY values are distinct', () => {
        const vals = Object.values(STATUS_BAR_PRIORITY);
        assert.strictEqual(vals.length, new Set(vals).size, 'Priority values must be unique');
    });

    test('SAVE_DEBOUNCE_MS is reasonable', () => {
        assert.ok(SAVE_DEBOUNCE_MS >= 100 && SAVE_DEBOUNCE_MS <= 2000);
    });
});
