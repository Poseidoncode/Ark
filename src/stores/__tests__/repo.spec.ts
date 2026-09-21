import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createPinia, setActivePinia } from 'pinia';
import { gitService, type DiffInfo, type RepoSnapshot } from '../../services/git';
import { useRepoStore } from '../repo';

vi.mock('../../services/git', () => ({
  gitService: {
    getDiff: vi.fn(),
    getCommitDiff: vi.fn(),
    getRepoSnapshot: vi.fn(),
    listStashes: vi.fn(),
    getHistory: vi.fn(),
  },
}));

const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => { resolve = r; });
  return { promise, resolve };
};

const repoInfo = (path: string) => ({
  path,
  current_branch: 'main',
  is_dirty: false,
  ahead: 0,
  behind: 0,
});

describe('repo store staleness guards', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
    useRepoStore().setRepoInfo(repoInfo('/r'));
  });

  it('drops an older file diff that resolves after a newer selection', async () => {
    const slow = deferred<DiffInfo[]>();
    const fast = deferred<DiffInfo[]>();
    vi.mocked(gitService.getDiff)
      .mockReturnValueOnce(slow.promise)
      .mockReturnValueOnce(fast.promise);

    const store = useRepoStore();
    const first = store.setSelectedFile('a.txt');
    const second = store.setSelectedFile('b.txt');

    fast.resolve([{ path: 'b.txt', additions: 1, deletions: 0, diff_text: 'b', truncated: false }]);
    await second;
    expect(store.diffs.map(d => d.path)).toEqual(['b.txt']);

    slow.resolve([{ path: 'a.txt', additions: 1, deletions: 0, diff_text: 'a', truncated: false }]);
    await first;
    expect(store.diffs.map(d => d.path)).toEqual(['b.txt']);
  });

  it('drops refresh results when the repo switched mid-flight', async () => {
    const snap = deferred<RepoSnapshot>();
    const stash = deferred<{ index: number; message: string; sha: string }[]>();
    vi.mocked(gitService.getRepoSnapshot).mockReturnValueOnce(snap.promise);
    vi.mocked(gitService.listStashes).mockReturnValueOnce(stash.promise);

    const store = useRepoStore();
    const pending = store.refreshRepo();
    store.setRepoInfo(repoInfo('/other'));

    snap.resolve({
      status: [{ path: 'x.txt', status: 'modified', staged: false }],
      branches: [],
      conflicts: [],
      info: repoInfo('/r'),
    });
    stash.resolve([]);
    await pending;

    expect(store.fileStatuses).toEqual([]);
    expect(store.repoInfo?.path).toBe('/other');
  });
});
