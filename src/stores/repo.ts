import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import { gitService, type RepositoryInfo, type FileStatus, type BranchInfo, type CommitInfo, type StashInfo, type ConflictInfo, type DiffInfo } from '../services/git';

// Memory optimization limits
const MAX_STASHES = 100;
const MAX_COMMITS = 200;

export const useRepoStore = defineStore('repo', () => {
  // State
  const repoInfo = ref<RepositoryInfo | null>(null);
  const fileStatuses = ref<FileStatus[]>([]);
  const branches = ref<BranchInfo[]>([]);
  const commits = ref<CommitInfo[]>([]);
  const commitsLoading = ref(false);
  const stashes = ref<StashInfo[]>([]);
  const conflicts = ref<ConflictInfo[]>([]);
  const recentRepoInfos = ref<RepositoryInfo[]>([]);
  const diffs = ref<DiffInfo[]>([]);

  // Per-section staleness + last error from the most recent refreshRepo().
  // A section is stale when its last refresh failed and the displayed data
  // may no longer reflect the repository state.
  const staleSections = ref<Record<string, boolean>>({});
  const lastRefreshErrors = ref<Record<string, string>>({});

  // Selection state
  const selectedFile = ref<string | null>(null);
  const selectedCommit = ref<CommitInfo | null>(null);
  const selectedCommitFile = ref<string | null>(null);

  // Computed
  const stagedFiles = computed(() => fileStatuses.value.filter(f => f.staged).map(f => f.path));
  const allStaged = computed(() => fileStatuses.value.length > 0 && fileStatuses.value.every(f => f.staged));

  const currentBranch = computed(() => {
    return branches.value.find((b: BranchInfo) => b.is_current)?.name || 'Unknown';
  });

  const getRecentRepoInfo = (path: string) => {
    return recentRepoInfos.value.find(r => r.path === path);
  };

  // Actions
  const setRepoInfo = (info: RepositoryInfo | null) => {
    repoInfo.value = info;
  };

  const setRecentRepoInfos = (infos: RepositoryInfo[]) => {
    recentRepoInfos.value = infos;
  };

  // Generation counters + repo-path guards discard stale async responses:
  // rapid selection changes, overlapping refreshes, or a repo switch
  // mid-flight must never let an older response overwrite newer state.
  let refreshGen = 0;
  let commitsGen = 0;
  let fileDiffGen = 0;
  let commitDiffGen = 0;

  const refreshRepo = async () => {
    if (!repoInfo.value) return;
    const path = repoInfo.value.path;
    const myGen = ++refreshGen;
    // One snapshot IPC (status/branches/conflicts/repoInfo, single backend
    // open + single status scan) plus the always-fresh stash list.
    const [snapshotResult, stashResult] = await Promise.allSettled([
      gitService.getRepoSnapshot(),
      gitService.listStashes(MAX_STASHES, 0),
    ]);
    // Stale: repo switched or a newer refresh started — drop silently.
    if (repoInfo.value?.path !== path || myGen !== refreshGen) return;
    const failures: string[] = [];
    const markFresh = (key: string) => {
      staleSections.value[key] = false;
      delete lastRefreshErrors.value[key];
    };
    const markStale = (key: string, reason: unknown) => {
      staleSections.value[key] = true;
      const message = reason instanceof Error ? reason.message : String(reason);
      lastRefreshErrors.value[key] = message;
      console.error(`Failed to refresh ${key}:`, reason);
      failures.push(`${key}: ${message}`);
    };
    if (snapshotResult.status === 'fulfilled') {
      const snap = snapshotResult.value;
      fileStatuses.value = snap.status;
      branches.value = snap.branches;
      conflicts.value = snap.conflicts;
      repoInfo.value = snap.info;
      markFresh('status');
      markFresh('branches');
      markFresh('conflicts');
      markFresh('repoInfo');
    } else {
      markStale('status', snapshotResult.reason);
      markStale('branches', snapshotResult.reason);
      markStale('conflicts', snapshotResult.reason);
      markStale('repoInfo', snapshotResult.reason);
    }
    if (stashResult.status === 'fulfilled') {
      stashes.value = stashResult.value;
      markFresh('stashes');
    } else {
      markStale('stashes', stashResult.reason);
    }
    if (failures.length > 0) {
      throw new Error(`Refresh incomplete: ${failures.join('; ')}`);
    }
  };

  const refreshCommits = async (limit: number = MAX_COMMITS, offset: number = 0) => {
    const path = repoInfo.value?.path;
    const myGen = ++commitsGen;
    commitsLoading.value = true;
    try {
      const list = await gitService.getHistory(limit, offset);
      if (repoInfo.value?.path !== path || myGen !== commitsGen) return;
      commits.value = list;
    } finally {
      // Only the latest call for the current repo may clear the flag.
      if (repoInfo.value?.path === path && myGen === commitsGen) {
        commitsLoading.value = false;
      }
    }
  };

  const setSelectedFile = async (filePath: string | null) => {
    selectedFile.value = filePath;
    if (!filePath) {
      diffs.value = [];
      return;
    }
    const path = repoInfo.value?.path;
    const myGen = ++fileDiffGen;
    try {
      const d = await gitService.getDiff(filePath);
      if (myGen !== fileDiffGen || repoInfo.value?.path !== path || selectedFile.value !== filePath) return;
      diffs.value = d;
    } catch (err) {
      if (myGen !== fileDiffGen || repoInfo.value?.path !== path || selectedFile.value !== filePath) return;
      console.error('Failed to get diff:', err);
      diffs.value = [];
    }
  };

  const setSelectedCommit = async (commit: CommitInfo | null) => {
    selectedCommit.value = commit;
    if (!commit) {
      diffs.value = [];
      selectedCommitFile.value = null;
      return;
    }
    const path = repoInfo.value?.path;
    const sha = commit.sha;
    const myGen = ++commitDiffGen;
    try {
      const d = await gitService.getCommitDiff(sha);
      if (myGen !== commitDiffGen || repoInfo.value?.path !== path || selectedCommit.value?.sha !== sha) return;
      diffs.value = d;
      selectedCommitFile.value = d.length > 0 ? d[0].path : null;
    } catch (err) {
      if (myGen !== commitDiffGen || repoInfo.value?.path !== path || selectedCommit.value?.sha !== sha) return;
      // A selection change must never throw into the watcher: a missing or
      // unreadable commit simply yields an empty diff view.
      console.warn('Commit diff unavailable:', err);
      diffs.value = [];
      selectedCommitFile.value = null;
    }
  };

  const clearSelection = () => {
    selectedFile.value = null;
    selectedCommit.value = null;
    selectedCommitFile.value = null;
    diffs.value = [];
  };

  return {
    // State
    repoInfo,
    fileStatuses,
    branches,
    commits,
    commitsLoading,
    stashes,
    conflicts,
    recentRepoInfos,
    diffs,
    staleSections,
    lastRefreshErrors,
    selectedFile,
    selectedCommit,
    selectedCommitFile,


    // Computed
    stagedFiles,
    allStaged,
    currentBranch,
    getRecentRepoInfo,

    // Actions
    setRepoInfo,
    setRecentRepoInfos,
    refreshRepo,
    refreshCommits,
    setSelectedFile,
    setSelectedCommit,
    clearSelection,
  };
});
