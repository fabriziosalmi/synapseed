/**
 * TTL cache with LRU eviction for CLI results.
 * Prevents hammering synapseed on rapid refreshes.
 */
import { CACHE_TTL, MAX_CACHE_ENTRIES } from './constants';

interface CacheEntry {
    data: unknown;
    expires: number;
}

export class DataCache {
    private store = new Map<string, CacheEntry>();
    private defaultTtlMs: number;
    private maxEntries: number;

    constructor(ttlMs: number = CACHE_TTL.DEFAULT, maxEntries: number = MAX_CACHE_ENTRIES) {
        this.defaultTtlMs = ttlMs;
        this.maxEntries = maxEntries;
    }

    get<T>(key: string): T | undefined {
        const entry = this.store.get(key);
        if (!entry) { return undefined; }
        if (Date.now() > entry.expires) {
            this.store.delete(key);
            return undefined;
        }
        // Move to end for LRU ordering
        this.store.delete(key);
        this.store.set(key, entry);
        return entry.data as T;
    }

    set<T>(key: string, data: T, ttlMs?: number): void {
        // Evict oldest entry if at capacity
        if (this.store.size >= this.maxEntries && !this.store.has(key)) {
            const oldest = this.store.keys().next().value;
            if (oldest !== undefined) { this.store.delete(oldest); }
        }
        this.store.set(key, {
            data,
            expires: Date.now() + (ttlMs ?? this.defaultTtlMs),
        });
    }

    invalidate(key?: string): void {
        if (key) {
            this.store.delete(key);
        } else {
            this.store.clear();
        }
    }

    get size(): number {
        return this.store.size;
    }
}

export const globalCache = new DataCache();
