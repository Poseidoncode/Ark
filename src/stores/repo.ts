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
    try {
      const [status, branchList, stashList, conflictList, info] = await Promise.all([
        gitService.getStatus(),
        gitService.getBranches(),
        gitService.listStashes(),
        gitService.getConflicts(),
        gitService.getCurrentRepoInfo()
      ]);
      fileStatuses.value = status;
      branches.value = branchList;
      stashes.value = stashList.slice(0, MAX_STASHES);
      conflicts.value = conflictList;
      if (info) {
        repoInfo.value = info;
      }
    } catch (err) {
      console.error('Failed to refresh repo:', err);
      throw err;
    }
  };

  const refreshCommits = async () => {
    commitsLoading.value = true;
    try {
      commits.value = await gitService.getHistory(MAX_COMMITS);
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
