import * as assert from 'assert';
import { DataCache } from '../../cache';

suite('DataCache', () => {
    test('get returns undefined for missing key', () => {
        const cache = new DataCache(1000);
        assert.strictEqual(cache.get('missing'), undefined);
    });

    test('set and get round-trip', () => {
        const cache = new DataCache(1000);
        cache.set('key1', { value: 42 });
        assert.deepStrictEqual(cache.get('key1'), { value: 42 });
    });

    test('invalidate removes single key', () => {
        const cache = new DataCache(1000);
        cache.set('a', 1);
        cache.set('b', 2);
        cache.invalidate('a');
        assert.strictEqual(cache.get('a'), undefined);
        assert.strictEqual(cache.get('b'), 2);
    });

    test('invalidate without args clears all', () => {
        const cache = new DataCache(1000);
        cache.set('a', 1);
        cache.set('b', 2);
        assert.strictEqual(cache.size, 2);
        cache.invalidate();
        assert.strictEqual(cache.size, 0);
    });

    test('TTL expiry', async () => {
        const cache = new DataCache(50); // 50ms TTL
        cache.set('fast', 'data');
        assert.strictEqual(cache.get('fast'), 'data');
        await new Promise(r => setTimeout(r, 60));
        assert.strictEqual(cache.get('fast'), undefined);
    });

    test('custom TTL per entry', async () => {
        const cache = new DataCache(5000); // 5s default
        cache.set('short', 'data', 50); // 50ms override
        assert.strictEqual(cache.get('short'), 'data');
        await new Promise(r => setTimeout(r, 60));
        assert.strictEqual(cache.get('short'), undefined);
    });

    test('LRU eviction at max entries', () => {
        const cache = new DataCache(60_000);
        // Fill beyond MAX_CACHE_ENTRIES (100)
        for (let i = 0; i < 105; i++) {
            cache.set(`key-${i}`, i);
        }
        // Oldest entries should be evicted
        assert.strictEqual(cache.get('key-0'), undefined);
        assert.strictEqual(cache.get('key-1'), undefined);
        // Recent entries should still be present
        assert.strictEqual(cache.get('key-104'), 104);
    });

    test('get refreshes LRU position', () => {
        const cache = new DataCache(60_000);
        // Fill to 99
        for (let i = 0; i < 99; i++) {
            cache.set(`key-${i}`, i);
        }
        // Access key-0 to refresh its position
        cache.get('key-0');
        // Add 2 more to trigger eviction
        cache.set('new-1', 'a');
        cache.set('new-2', 'b');
        // key-0 should survive (was refreshed), key-1 should be evicted
        assert.strictEqual(cache.get('key-0'), 0);
        assert.strictEqual(cache.get('key-1'), undefined);
    });
});
