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

  const refreshRepo = async () => {
    if (!repoInfo.value) return;
    const sections = [
      {
        key: 'status',
        apply: (v: FileStatus[]) => { fileStatuses.value = v; },
        load: () => gitService.getStatus(),
      },
      {
        key: 'branches',
        apply: (v: BranchInfo[]) => { branches.value = v; },
        load: () => gitService.getBranches(),
      },
      {
        key: 'stashes',
        apply: (v: StashInfo[]) => { stashes.value = v; },
        load: () => gitService.listStashes(MAX_STASHES, 0),
      },
      {
        key: 'conflicts',
        apply: (v: ConflictInfo[]) => { conflicts.value = v; },
        load: () => gitService.getConflicts(),
      },
      {
        key: 'repoInfo',
        apply: (v: RepositoryInfo | null) => { if (v) repoInfo.value = v; },
        load: () => gitService.getCurrentRepoInfo(),
      },
    ] as const;
    const results = await Promise.allSettled(sections.map((s) => s.load()));
    const failures: string[] = [];
    results.forEach((r, i) => {
      const section = sections[i];
      if (r.status === 'fulfilled') {
        (section.apply as (v: unknown) => void)(r.value);
        staleSections.value[section.key] = false;
        delete lastRefreshErrors.value[section.key];
      } else {
        // Partial failure: keep the previous data but mark it stale and
        // record the error instead of silently showing outdated state.
        staleSections.value[section.key] = true;
        const message = r.reason instanceof Error ? r.reason.message : String(r.reason);
        lastRefreshErrors.value[section.key] = message;
        console.error(`Failed to refresh ${section.key}:`, r.reason);
        failures.push(`${section.key}: ${message}`);
      }
    });
    if (failures.length > 0) {
      throw new Error(`Refresh incomplete (${failures.length}/${sections.length} failed): ${failures.join('; ')}`);
    }
  };

  const refreshCommits = async (limit: number = MAX_COMMITS, offset: number = 0) => {
    commitsLoading.value = true;
    try {
      commits.value = await gitService.getHistory(limit, offset);
    } finally {
      commitsLoading.value = false;
    }
  };

  const setSelectedFile = async (filePath: string | null) => {
    selectedFile.value = filePath;
    if (filePath) {
      try {
        const d = await gitService.getDiff(filePath);
        diffs.value = d;
      } catch (err) {
        console.error('Failed to get diff:', err);
        diffs.value = [];
      }
    } else {
      diffs.value = [];
    }
  };

  const setSelectedCommit = async (commit: CommitInfo | null) => {
    selectedCommit.value = commit;
    if (commit) {
      try {
        const d = await gitService.getCommitDiff(commit.sha);
        diffs.value = d;
        if (d.length > 0) {
          selectedCommitFile.value = d[0].path;
        } else {
          selectedCommitFile.value = null;
        }
      } catch (err) {
        const errMsg = String(err);
        if (errMsg.includes("Commit not found")) {
          console.warn("Selected commit not found, diff unavailable:", err);
          diffs.value = [];
          selectedCommitFile.value = null;
        } else {
          throw err;
        }
      }
    } else {
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
