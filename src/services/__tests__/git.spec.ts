import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { gitService } from '../git';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

beforeEach(() => {
  vi.clearAllMocks();
  gitService.invalidate(); // clear internal cache
});

describe('GitService', () => {
  describe('Caching', () => {
    it('should cache repo:status within TTL', async () => {
      vi.mocked(invoke).mockResolvedValue([]);

      const result1 = await gitService.getStatus();
      const result2 = await gitService.getStatus();

      // getStatus is cached — second call should not invoke
      expect(invoke).toHaveBeenCalledTimes(1);
      expect(result1).toEqual(result2);
    });

    it('should invalidate cache on mutation operations', async () => {
      vi.mocked(invoke).mockResolvedValue([]);

      // Prime cache
      await gitService.getStatus();
      expect(invoke).toHaveBeenCalledTimes(1);

      // Stage files invalidates repo:status + diff:
      vi.mocked(invoke).mockResolvedValue({ staged: ['file.txt'], warnings: [] });
      await gitService.stageFiles(['file.txt']);

      // Re-fetch — should invoke again (cache was invalidated)
      expect(invoke).toHaveBeenCalledTimes(2);
      vi.mocked(invoke).mockResolvedValue([{ path: 'file.txt', status: 'M', staged: true }]);
      await gitService.getStatus();
      expect(invoke).toHaveBeenCalledTimes(3); // 1st getStatus + stageFiles + 2nd getStatus
    });
  });

  describe('Cache Invalidation', () => {
    it('should invalidate diff: keys on stageFiles', async () => {
      vi.mocked(invoke).mockResolvedValue('diff text');

      await gitService.getDiff('file.txt');

      vi.mocked(invoke).mockResolvedValue({ staged: ['file.txt'], warnings: [] });
      await gitService.stageFiles(['file.txt']);

      // Diff cache was invalidated — must re-fetch
      vi.mocked(invoke).mockResolvedValue('new diff');
      await gitService.getDiff('file.txt');

      const diffCalls = vi.mocked(invoke).mock.calls.filter(c => c[0] === 'get_diff');
      expect(diffCalls).toHaveLength(2); // initial + after stage
    });

    it('should NOT invalidate diff: keys with repo: prefix', async () => {
      vi.mocked(invoke).mockResolvedValue('diff text');

      await gitService.getDiff('file.txt');

      // fetch invalidates repo: but not diff:
      vi.mocked(invoke).mockResolvedValue(undefined);
      await gitService.fetch();

      // Diff should still be cached (no new invoke)
      vi.mocked(invoke).mockResolvedValue('diff text');
      await gitService.getDiff('file.txt');

      const diffCalls = vi.mocked(invoke).mock.calls.filter(c => c[0] === 'get_diff');
      expect(diffCalls).toHaveLength(1); // only the initial call
    });
  });

  describe('Stale Cache Fallback', () => {
    it('should throw when no stale cache available on first call', async () => {
      vi.mocked(invoke).mockRejectedValue(new Error('Backend error'));

      await expect(gitService.getStatus()).rejects.toThrow('Backend error');
    });

    it('should return stale cache on backend error', async () => {
      vi.mocked(invoke).mockResolvedValue([{ path: 'file.txt', status: ' M', staged: true }]);
      const result1 = await gitService.getStatus();
      expect(result1).toEqual([{ path: 'file.txt', status: ' M', staged: true }]);

      // Second call fails — should return stale cache
      vi.mocked(invoke).mockRejectedValue(new Error('Backend error'));
      const result2 = await gitService.getStatus();
      expect(result2).toEqual([{ path: 'file.txt', status: ' M', staged: true }]);
    });
  });
});
