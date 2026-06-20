import { describe, it, expect, vi, beforeEach } from 'vitest';  // ponytail: afterEach unused

// Test the GitServiceOptimizer class directly

interface CacheEntry<T> {
  data: T;
  expiresAt: number;
}

class GitServiceOptimizer {
  private cache = new Map<string, CacheEntry<unknown>>();
  private readonly DEFAULT_TTL = 5000;
  // ponytail: removed unused SHORT_TTL, LONG_TTL
  private readonly MAX_CACHE_SIZE = 50;

  private evictOldCache() {
    if (this.cache.size > this.MAX_CACHE_SIZE) {
      const entries = Array.from(this.cache.entries());
      entries.sort((a, b) => a[1].expiresAt - b[1].expiresAt);
      const toRemove = Math.floor(this.MAX_CACHE_SIZE * 0.25);
      for (let i = 0; i < toRemove; i++) {
        this.cache.delete(entries[i][0]);
      }
    }
  }

  private async getCachedOrFetch<T>(
    key: string,
    fetchFn: () => Promise<T>,
    ttl: number = this.DEFAULT_TTL
  ): Promise<T> {
    const now = Date.now();
    const cached = this.cache.get(key) as CacheEntry<T> | undefined;

    if (cached && cached.expiresAt > now) {
      return cached.data;
    }

    try {
      const data = await fetchFn();
      this.cache.set(key, { data, expiresAt: now + ttl });
      this.evictOldCache();
      return data;
    } catch (error) {
      const cached = this.cache.get(key) as CacheEntry<T> | undefined;
      if (cached) {
        console.warn(`Cache fallback for ${key}:`, error);
        return cached.data;
      }
      throw error;
    }
  }

  invalidate(keyPattern?: string): void {
    if (!keyPattern) {
      this.cache.clear();
      return;
    }

    for (const key of this.cache.keys()) {
      if (key.includes(keyPattern)) {
        this.cache.delete(key);
      }
    }
  }

  // Test helper methods
  getCacheSize(): number {
    return this.cache.size;
  }

  getCacheEntry<T>(key: string): CacheEntry<T> | undefined {
    return this.cache.get(key) as CacheEntry<T> | undefined;
  }

  // Test method that uses caching
  async fetchData<T>(key: string, fetchFn: () => Promise<T>, ttl: number = this.DEFAULT_TTL): Promise<T> {
    return this.getCachedOrFetch(key, fetchFn, ttl);
  }
}

describe('GitServiceOptimizer', () => {
  let optimizer: GitServiceOptimizer;

  beforeEach(() => {
    optimizer = new GitServiceOptimizer();
  });

  describe('Caching', () => {
    it('should return cached data within TTL', async () => {
      const fetchFn = vi.fn().mockResolvedValue('fresh data');
      
      // First call - should fetch
      const result1 = await optimizer.fetchData('test:key', fetchFn, 5000);
      expect(result1).toBe('fresh data');
      expect(fetchFn).toHaveBeenCalledTimes(1);

      // Second call within TTL - should return cached
      const result2 = await optimizer.fetchData('test:key', fetchFn, 5000);
      expect(result2).toBe('fresh data');
      expect(fetchFn).toHaveBeenCalledTimes(1); // Still only called once
    });

    it('should fetch new data after TTL expires', async () => {
      vi.useFakeTimers();
      
      const fetchFn = vi.fn().mockResolvedValue('data');
      
      // First call
      await optimizer.fetchData('test:key', fetchFn, 1000);
      expect(fetchFn).toHaveBeenCalledTimes(1);

      // Advance time past TTL
      vi.advanceTimersByTime(2000);

      // Should fetch again
      await optimizer.fetchData('test:key', fetchFn, 1000);
      expect(fetchFn).toHaveBeenCalledTimes(2);

      vi.useRealTimers();
    });

    it('should cache different keys independently', async () => {
      const fetchFn1 = vi.fn().mockResolvedValue('data1');
      const fetchFn2 = vi.fn().mockResolvedValue('data2');

      const result1 = await optimizer.fetchData('key1', fetchFn1);
      const result2 = await optimizer.fetchData('key2', fetchFn2);

      expect(result1).toBe('data1');
      expect(result2).toBe('data2');
      expect(fetchFn1).toHaveBeenCalledTimes(1);
      expect(fetchFn2).toHaveBeenCalledTimes(1);
    });
  });

  describe('Cache Invalidation', () => {
    it('should clear all cache when invalidate() is called without pattern', async () => {
      const fetchFn = vi.fn().mockResolvedValue('data');

      await optimizer.fetchData('key1', fetchFn);
      await optimizer.fetchData('key2', fetchFn);
      
      expect(optimizer.getCacheSize()).toBe(2);

      optimizer.invalidate();

      expect(optimizer.getCacheSize()).toBe(0);
    });

    it('should invalidate cache entries matching pattern', async () => {
      const fetchFn = vi.fn().mockResolvedValue('data');

      await optimizer.fetchData('repo:status', fetchFn);
      await optimizer.fetchData('repo:branches', fetchFn);
      await optimizer.fetchData('diff:file', fetchFn);
      
      expect(optimizer.getCacheSize()).toBe(3);

      optimizer.invalidate('repo:');

      expect(optimizer.getCacheSize()).toBe(1); // Only diff:file should remain
      expect(optimizer.getCacheEntry('diff:file')).toBeDefined();
    });

    it('should not invalidate entries that do not match pattern', async () => {
      const fetchFn = vi.fn().mockResolvedValue('data');

      await optimizer.fetchData('repo:status', fetchFn);
      await optimizer.fetchData('settings', fetchFn);
      
      optimizer.invalidate('repo:');

      expect(optimizer.getCacheSize()).toBe(1);
      expect(optimizer.getCacheEntry('settings')).toBeDefined();
    });
  });

  describe('Stale Cache Fallback', () => {
    it('should return stale cache on error if available', async () => {
      vi.useFakeTimers();
      
      let callCount = 0;
      const fetchFn = vi.fn().mockImplementation(() => {
        callCount++;
        if (callCount === 1) {
          return Promise.resolve('initial data');
        }
        return Promise.reject(new Error('Network error'));
      });

      // First call - populate cache
      const result1 = await optimizer.fetchData('test:key', fetchFn, 1000);
      expect(result1).toBe('initial data');
      expect(fetchFn).toHaveBeenCalledTimes(1);

      // Advance time past TTL so cache is stale
      vi.advanceTimersByTime(2000);

      // Second call - should fail but return stale cache
      const result2 = await optimizer.fetchData('test:key', fetchFn, 1000);
      expect(result2).toBe('initial data'); // Returns stale cache
      expect(fetchFn).toHaveBeenCalledTimes(2);

      vi.useRealTimers();
    });

    it('should throw error when no stale cache available', async () => {
      const fetchFn = vi.fn().mockRejectedValue(new Error('Network error'));

      await expect(optimizer.fetchData('test:key', fetchFn)).rejects.toThrow('Network error');
    });
  });

  describe('Cache Eviction', () => {
    it('should evict old cache entries when limit is reached', async () => {
      // Create optimizer with small cache size for testing
      const smallOptimizer = new GitServiceOptimizer();
      
      // Fill cache beyond limit (MAX_CACHE_SIZE = 50)
      const fetchFn = vi.fn().mockImplementation((i: number) => Promise.resolve(`data${i}`));

      for (let i = 0; i < 60; i++) {
        await smallOptimizer.fetchData(`key:${i}`, () => fetchFn(i), 5000);
      }

      // Cache should have been evicted
      expect(smallOptimizer.getCacheSize()).toBeLessThanOrEqual(50);
    });
  });
});
