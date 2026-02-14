/**
 * TTL cache for CLI results. Prevents hammering synapseed on rapid refreshes.
 */
export class DataCache {
    private store = new Map<string, { data: any; expires: number }>();
    private defaultTtlMs: number;

    constructor(ttlMs = 10_000) {
        this.defaultTtlMs = ttlMs;
    }

    get<T>(key: string): T | undefined {
        const entry = this.store.get(key);
        if (!entry) { return undefined; }
        if (Date.now() > entry.expires) {
            this.store.delete(key);
            return undefined;
        }
        return entry.data as T;
    }

    set<T>(key: string, data: T, ttlMs?: number): void {
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

export const globalCache = new DataCache(15_000);
