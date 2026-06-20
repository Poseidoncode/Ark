<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, computed } from 'vue';
import { gitService, type RepositoryInfo, type FileStatus, type BranchInfo, type CommitInfo, type StashInfo, type ConflictInfo, type SettingsPayload, type DiffInfo, type StageResult, type TagInfo, type RemoteInfo } from './services/git';
import { open, ask } from '@tauri-apps/plugin-dialog';
import { useToast } from './composables/useToast';
import Toast from './components/Toast.vue';
import { useContextMenu } from './composables/useContextMenu';
import ContextMenu from './components/ContextMenu.vue';
import { useKeyboardShortcuts } from './composables/useKeyboardShortcuts';
import { openPath, openUrl } from '@tauri-apps/plugin-opener';
import { listen } from '@tauri-apps/api/event';
import { homeDir } from '@tauri-apps/api/path';
import DiffViewer from './components/DiffViewer.vue';

const errMsg = (e: unknown): string => e instanceof Error ? e.message : String(e);

let homeDirCache: string | null = null;
const MAX_STASHES = 100;
const MAX_COMMITS = 200;

const repoInfo = ref<RepositoryInfo | null>(null);
const fileStatuses = ref<FileStatus[]>([]);
const branches = ref<BranchInfo[]>([]);
const commits = ref<CommitInfo[]>([]);
const stashes = ref<StashInfo[]>([]);
const conflicts = ref<ConflictInfo[]>([]);
const settings = ref<SettingsPayload | null>(null);
const recentRepoInfos = ref<RepositoryInfo[]>([]);
const diffs = ref<DiffInfo[]>([]);

const commitMessage = ref("");
const selectedFile = ref<string | null>(null);
const selectedCommit = ref<CommitInfo | null>(null);
const selectedCommitFile = ref<string | null>(null);
const view = ref<"changes" | "history" | "stashes" | "conflicts">("changes");
const loading = ref(false);
const loadingMessage = ref("");
const isMajorOperation = ref(false);
const error = ref<string | null>(null);
const toast = useToast();
const { showContextMenu, hideContextMenu, isVisible, position, menuItems } = useContextMenu();

// Modal State
const showCloneModal = ref(false);
const cloneUrl = ref("");
const clonePath = ref("");
const showSettingsModal = ref(false);
const showBranchModal = ref(false);
const newBranchName = ref("");
const showRecentRepos = ref(false);
const showTagsModal = ref(false);
const showRemotesModal = ref(false);
const tags = ref<TagInfo[]>([]);
const remotes = ref<RemoteInfo[]>([]);
const newRemoteName = ref("");
const newRemoteUrl = ref("");
const amendCommit = ref(false);
const searchCommitQuery = ref("");
const searchCommitQueryDebounced = ref("");

// ponytail: simple 300ms debounce via watcher
let searchDebounceId: ReturnType<typeof setTimeout> | null = null;
watch(searchCommitQuery, (val) => {
  if (searchDebounceId) clearTimeout(searchDebounceId);
  searchDebounceId = setTimeout(() => { searchCommitQueryDebounced.value = val; }, 300);
});

let refreshTimer: ReturnType<typeof setTimeout> | null = null;
// ponytail: single refresh guard, no global operation lock
let refreshInProgress = false;

const clearError = () => { error.value = null; };

watch(showRecentRepos, async (isOpen) => {
  if (isOpen && settings.value?.recent_repositories.length) {
    try {
      const infos = await gitService.getRepositoriesInfo(settings.value?.recent_repositories || []);
      recentRepoInfos.value = infos;
    } catch (err) {
      console.error("Failed to fetch recent repo infos", err);
      error.value = errMsg(err);
    }
  }
});

const dropdownRef = ref<HTMLElement | null>(null);

const getRepoName = (path: string) => {
  if (!path || path.trim() === "") return "";
  const cleanPath = path.replace(/[/\\]+$/, '');
  if (cleanPath.endsWith('.git')) {
    const withoutGit = cleanPath.slice(0, -4).replace(/[/\\]+$/, '');
    const parts = withoutGit.split(/[/\\]/);
    return parts[parts.length - 1] || "";
  }
  const parts = cleanPath.split(/[/\\]/);
  const lastPart = parts[parts.length - 1];
  if (!lastPart || lastPart === '.' || lastPart === '..') {
    return parts[parts.length - 2] || path;
  }
  return lastPart || path;
};

const currentProjectName = computed(() => {
  if (!repoInfo.value) return "";
  const path = repoInfo.value.path;
  const name = getRepoName(path);
  if (name === repoInfo.value.current_branch) {
    if (path && (path.includes('/') || path.includes('\\\\'))) return getRepoName(path);
    return path || "";
  }
  return name;
});

watch(currentProjectName, (name) => {
  document.title = name ? `Ark - ${name}` : "Ark";
}, { immediate: true });

const stagedFiles = computed(() => fileStatuses.value.filter(f => f.staged).map(f => f.path));
const allStaged = computed(() => fileStatuses.value.length > 0 && fileStatuses.value.every(f => f.staged));

const filteredCommits = computed(() => {
  const q = searchCommitQueryDebounced.value.toLowerCase().trim();
  if (!q) return commits.value;
  return commits.value.filter(c =>
    c.message.toLowerCase().includes(q) ||
    c.sha.toLowerCase().includes(q) ||
    c.author.toLowerCase().includes(q)
  );
});

const getRecentRepoInfo = (path: string) => {
  return recentRepoInfos.value.find(r => r.path === path);
};

const getCurrentBranch = () => {
  return branches.value.find((b: BranchInfo) => b.is_current)?.name || 'Unknown';
};

const fetchSettings = async () => {
  try {
    const s = await gitService.getSettings();
    settings.value = s;
  } catch (err) {
    console.error("Failed to fetch settings", err);
    error.value = errMsg(err);
  }
};

const refreshRepo = async (scope: 'full' | 'status' = 'full') => {
  if (!repoInfo.value || refreshInProgress) return;
  refreshInProgress = true;
  try {
    if (scope === 'status') {
      const results = await Promise.allSettled([
        gitService.getStatus(),
        gitService.getBranches(),
        gitService.getCurrentRepoInfo()
      ]);
      const [statusRes, branchRes, infoRes] = results;
      if (statusRes.status === 'fulfilled') fileStatuses.value = statusRes.value;
      if (branchRes.status === 'fulfilled') branches.value = branchRes.value;
      if (infoRes.status === 'fulfilled' && infoRes.value) repoInfo.value = infoRes.value;

      const errors = results
        .filter((r): r is PromiseRejectedResult => r.status === 'rejected')
        .map(r => String(r.reason));
      if (errors.length > 0) console.warn('Partial refresh failures:', errors);
      return;
    }

    const results = await Promise.allSettled([
      gitService.getStatus(),
      gitService.getBranches(),
      gitService.listStashes(),
      gitService.getConflicts(),
      gitService.getCurrentRepoInfo()
    ]);

    const [statusRes, branchRes, stashRes, conflictRes, infoRes] = results;

    if (statusRes.status === 'fulfilled') fileStatuses.value = statusRes.value;
    if (branchRes.status === 'fulfilled') branches.value = branchRes.value;
    if (stashRes.status === 'fulfilled') stashes.value = stashRes.value.slice(0, MAX_STASHES);
    if (conflictRes.status === 'fulfilled') conflicts.value = conflictRes.value;
    if (infoRes.status === 'fulfilled' && infoRes.value) repoInfo.value = infoRes.value;

    const conflictResult = conflictRes.status === 'fulfilled' ? conflictRes.value : [];
    if (conflictResult.length > 0 && view.value !== "conflicts") {
      view.value = "conflicts";
    }
    if (view.value === "history" && commits.value.length === 0) {
      try {
        const history = await gitService.getHistory(50);
        commits.value = history.slice(0, MAX_COMMITS);
      } catch (err) {
        const msg = errMsg(err);
        if (!msg.includes("Failed to push HEAD")) throw err;
        commits.value = [];
      }
    }

    const errors = results
      .filter((r): r is PromiseRejectedResult => r.status === 'rejected')
      .map(r => String(r.reason));
    if (errors.length > 0) console.warn('Partial refresh failures:', errors);
  } catch (err) {
    error.value = errMsg(err);
  } finally {
    refreshInProgress = false;
  }
};

let unlisten: (() => void) | null = null;

onMounted(async () => {
  window.addEventListener('click', handleClickOutside);
  await fetchSettings();
  try {
    const info = await gitService.getCurrentRepoInfo();
    if (info) repoInfo.value = info;
  } catch (err) {
    console.error("Failed to fetch initial repo info", err);
    error.value = errMsg(err);
  }

  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  unlisten = await listen('git-state-changed', () => {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => { refreshRepo(); }, 200);
  });
});

onUnmounted(() => {
  if (unlisten) unlisten();
  window.removeEventListener('click', handleClickOutside);
  if (searchDebounceId) clearTimeout(searchDebounceId);
  if (refreshTimer) clearTimeout(refreshTimer);
});

watch([() => repoInfo.value?.path, view], ([newPath]) => {
  if (newPath) refreshRepo();
});

watch(amendCommit, (newVal) => {
  if (newVal && commits.value.length > 0) {
    if (!commitMessage.value || commitMessage.value.endsWith(' (amend)')) {
      commitMessage.value = commits.value[0].message + ' (amend)';
    }
  } else if (!newVal) {
    const suffix = ' (amend)';
    if (commitMessage.value && commitMessage.value.endsWith(suffix)) {
      commitMessage.value = commitMessage.value.slice(0, -suffix.length);
    }
  }
});

watch(selectedFile, (newFile: string | null) => {
  if (newFile && view.value === "changes") {
    gitService.getDiff(newFile).then(d => { diffs.value = d; }).catch(err => {
      console.warn('Diff fetch failed:', err);
    });
  } else if (!newFile && view.value === "changes") {
    diffs.value = [];
  }
});

watch(selectedCommit, async (newCommit) => {
  if (newCommit) {
    try {
      const d = await gitService.getCommitDiff(newCommit.sha);
      diffs.value = d;
      selectedCommitFile.value = d.length > 0 ? d[0].path : null;
    } catch (err) {
      if (errMsg(err).includes("Commit not found")) {
        diffs.value = [];
        selectedCommitFile.value = null;
      } else {
        error.value = errMsg(err);
      }
    }
  } else {
    diffs.value = [];
    selectedCommitFile.value = null;
  }
});

watch(cloneUrl, async (newUrl) => {
  if (newUrl) {
    const cleanUrl = newUrl.replace(/\/+$/, '');
    const match = cleanUrl.match(/\/([^\/]+?)(\.git)?$/);
    if (match && match[1]) {
      const repoName = match[1];
      let basePath = "";
      if (repoInfo.value) {
        const idx = repoInfo.value.path.lastIndexOf('/');
        basePath = idx > 0 ? repoInfo.value.path.substring(0, idx) : repoInfo.value.path;
      } else if (settings.value && settings.value.recent_repositories.length > 0) {
        const lastRepo = settings.value.recent_repositories[0];
        const idx = lastRepo.lastIndexOf('/');
        basePath = idx > 0 ? lastRepo.substring(0, idx) : lastRepo;
      }
      if (!basePath || !basePath.includes('github')) {
        try {
          if (!homeDirCache) homeDirCache = await homeDir();
          basePath = `${homeDirCache}/Documents/github`;
        } catch { basePath = ""; }
      }
      clonePath.value = `${basePath}/${repoName}`;
    }
  }
});

// ─── Context Menus ──────────────────────────────────────

const onCommitContextMenu = (event: MouseEvent, commit: CommitInfo) => {
  showContextMenu(event, [
    {
      label: 'Copy SHA', action: async () => {
        await navigator.clipboard.writeText(commit.sha.substring(0, 7));
        toast.success('SHA copied', { title: 'Copied' });
      }
    },
    {
      label: 'Copy Full SHA', action: async () => {
        await navigator.clipboard.writeText(commit.sha);
        toast.success('Full SHA copied', { title: 'Copied' });
      }
    },
    {
      label: 'Copy Commit Message', action: async () => {
        await navigator.clipboard.writeText(commit.message);
        toast.success('Message copied', { title: 'Copied' });
      }
    },
    { divider: true },
    { label: 'Create Branch from Commit', action: async () => {
      const name = prompt(`Enter new branch name from ${commit.sha.substring(0, 7)}:`);
      if (name?.trim()) {
        try {
          loading.value = true;
          await gitService.createBranch(name.trim(), commit.sha);
          await refreshRepo();
          toast.success(`Created branch ${name}`, { title: 'Success' });
        } catch (e) { error.value = errMsg(e); }
        finally { loading.value = false; }
      }
    }},
    { label: 'Create Tag', action: async () => {
      const tagName = prompt(`Enter tag name for ${commit.sha.substring(0, 7)}:`);
      if (tagName?.trim()) {
        const tagMessage = prompt(`Enter tag message (optional):`) || '';
        try {
          loading.value = true;
          await gitService.createTag(tagName.trim(), tagMessage, commit.sha);
          await refreshRepo();
          toast.success(`Created tag ${tagName}`, { title: 'Success' });
        } catch (e) { error.value = errMsg(e); }
        finally { loading.value = false; }
      }
    }},
    { label: 'Cherry-pick Commit', action: () => handleCherryPick(commit.sha) },
    { label: 'Revert Commit', danger: true, action: () => handleRevertCommit(commit.sha) },
    { label: 'Reset Branch to this Commit', danger: true, action: async () => {
      const confirmed = await ask(`Reset current branch to ${commit.sha.substring(0, 7)}? This will discard all commits after this point.`, { title: 'Reset Branch', kind: 'warning' });
      if (!confirmed) return;
      try {
        loading.value = true;
        loadingMessage.value = "Resetting branch...";
        isMajorOperation.value = true;
        await gitService.resetBranch(commit.sha);
        await refreshRepo();
        toast.success("Branch reset successfully", { title: 'Success' });
        error.value = null;
      } catch (e) { error.value = errMsg(e); }
      finally { loading.value = false; loadingMessage.value = ""; isMajorOperation.value = false; }
    }},
    { label: 'Merge into Current Branch', action: async () => {
      const confirmed = await ask(`Merge commit ${commit.sha.substring(0, 7)} into current branch?`, { title: 'Merge Commit', kind: 'info' });
      if (!confirmed) return;
      try {
        loading.value = true;
        loadingMessage.value = "Merging commit...";
        isMajorOperation.value = true;
        await gitService.mergeCommit(commit.sha);
        await refreshRepo();
        toast.success("Merge successful", { title: 'Success' });
        error.value = null;
      } catch (e) { error.value = errMsg(e); }
      finally { loading.value = false; loadingMessage.value = ""; isMajorOperation.value = false; }
    }},
    { divider: true },
    { label: 'View on GitHub', action: async () => {
      try {
        let url = await gitService.getRemoteUrl("origin");
        if (url) {
          if (url.startsWith('git@github.com:')) url = url.replace('git@github.com:', 'https://github.com/').replace(/\.git$/, '');
          else if (url.startsWith('https://')) url = url.replace(/\.git$/, '');
          await openUrl(`${url}/commit/${commit.sha}`);
        } else { toast.error("No origin remote found", { title: "Error" }); }
      } catch (e) { error.value = errMsg(e); }
    }},
    { divider: true },
    { label: 'Reveal in Finder/Explorer', action: async () => {
      if (repoInfo.value) {
        try { await gitService.revealInFinder(repoInfo.value.path); }
        catch (e) { error.value = errMsg(e); }
      }
    }}
  ]);
};

const onFileContextMenu = (event: MouseEvent, file: FileStatus) => {
  showContextMenu(event, [
    { label: file.staged ? 'Unstage File' : 'Stage File', action: () => toggleStaged(file) },
    { divider: true },
    { label: 'Discard Changes', danger: true, action: () => handleDiscardChanges(file.path) },
    { divider: true },
    { label: 'Copy Path', action: async () => {
      try { await navigator.clipboard.writeText(file.path); toast.success('Path copied', { title: 'Copied' }); }
      catch (err) { console.error('Clipboard error:', err); }
    }},
    { label: 'Ignore File', action: async () => {
      try { await gitService.addToGitignore(file.path); await refreshRepo('status'); toast.success(`Added ${file.path} to .gitignore`, { title: 'Success' }); }
      catch (e) { error.value = errMsg(e); }
    }},
    { label: 'View File History', action: async () => { view.value = "history"; searchCommitQuery.value = file.path; }},
    { label: 'Copy File Contents', action: async () => {
      try {
        if (repoInfo.value) {
          const content = await gitService.readFile(`${repoInfo.value.path}/${file.path}`);
          await navigator.clipboard.writeText(content);
          toast.success('File contents copied', { title: 'Copied' });
        }
      } catch (e) { error.value = errMsg(e); }
    }},
    { divider: true },
    { label: 'Reveal in Finder/Explorer', action: async () => {
      if (repoInfo.value) { try { await gitService.revealInFinder(`${repoInfo.value.path}/${file.path}`); } catch (e) { error.value = errMsg(e); } }
    }},
    { label: 'Open in Editor', action: async () => {
      if (repoInfo.value) { try { await openPath(`${repoInfo.value.path}/${file.path}`); } catch (e) { error.value = errMsg(e); } }
    }}
  ]);
};

const onFileHeaderContextMenu = (event: MouseEvent) => {
  if (fileStatuses.value.length === 0) return;
  showContextMenu(event, [
    { label: allStaged.value ? 'Unstage All' : 'Stage All', action: () => toggleAllStaged() },
    { divider: true },
    { label: 'Discard All Changes', danger: true, action: () => handleDiscardAllChanges() }
  ]);
};

const onCommitFileContextMenu = (event: MouseEvent, filePath: string) => {
  showContextMenu(event, [
    { label: 'Copy Path', action: async () => { await navigator.clipboard.writeText(filePath); toast.success('Path copied', { title: 'Copied' }); }},
    { label: 'View File History', action: async () => { searchCommitQuery.value = filePath; }},
    { divider: true },
    { label: 'Reveal in Finder/Explorer', action: async () => { if (repoInfo.value) { try { await gitService.revealInFinder(`${repoInfo.value.path}/${filePath}`); } catch (e) { error.value = errMsg(e); } } }},
    { label: 'Open in Editor', action: async () => { if (repoInfo.value) { try { await openPath(`${repoInfo.value.path}/${filePath}`); } catch (e) { error.value = errMsg(e); } } }}
  ]);
};

const onStashContextMenu = (event: MouseEvent, stash: StashInfo) => {
  showContextMenu(event, [
    { label: 'Apply Stash', action: () => handleApplyStash(stash.index) },
    { label: 'Pop Stash', action: () => handleStashPop(stash.index) },
    { label: 'Drop Stash', danger: true, action: () => handleDropStash(stash.index) },
    { divider: true },
    { label: 'Create Branch from Stash', action: () => handleBranchFromStash(stash.index) },
    { label: 'Copy SHA', action: async () => { await navigator.clipboard.writeText(stash.sha); toast.success('SHA copied', { title: 'Copied' }); }}
  ]);
};

const onConflictContextMenu = (event: MouseEvent, conflict: ConflictInfo) => {
  showContextMenu(event, [
    { label: 'Use Ours', action: () => handleResolve(conflict.path, true) },
    { label: 'Use Theirs', action: () => handleResolve(conflict.path, false) },
    { divider: true },
    { label: 'Copy Path', action: async () => { await navigator.clipboard.writeText(conflict.path); toast.success('Path copied', { title: 'Copied' }); }},
    { label: 'Reveal in Finder/Explorer', action: async () => { if (repoInfo.value) { try { await gitService.revealInFinder(`${repoInfo.value.path}/${conflict.path}`); } catch (e) { error.value = errMsg(e); } } }},
    { label: 'Open in Editor', action: async () => { if (repoInfo.value) { try { await openPath(`${repoInfo.value.path}/${conflict.path}`); } catch (e) { error.value = errMsg(e); } } }}
  ]);
};

// ─── Action Handlers ─────────────────────────────────────

const toggleAllStaged = async () => {
  if (loading.value || fileStatuses.value.length === 0) return;
  try {
    const paths = fileStatuses.value.map(f => f.path);
    if (allStaged.value) {
      await gitService.unstageFiles(paths);
    } else {
      error.value = null;
      const result: StageResult = await gitService.stageFiles(paths);
      if (result.warnings.length > 0) error.value = result.warnings.join('\n');
    }
    await refreshRepo();
  } catch (err) { error.value = errMsg(err); }
};

const toggleStaged = async (file: FileStatus) => {
  if (loading.value) return;
  try {
    if (file.staged) {
      await gitService.unstageFiles([file.path]);
    } else {
      error.value = null;
      const result: StageResult = await gitService.stageFiles([file.path]);
      if (result.warnings.length > 0) error.value = result.warnings.join('\n');
    }
    await refreshRepo();
  } catch (err) { error.value = errMsg(err); }
};

const handleCommit = async () => {
  if (loading.value) return;
  if (!commitMessage.value.trim()) { toast.error("Please enter a commit message", { title: "Commit Error" }); return; }
  if (!amendCommit.value && stagedFiles.value.length === 0) { toast.error("Please select files to commit", { title: "Commit Error" }); return; }
  try {
    loading.value = true;
    loadingMessage.value = "Creating commit...";
    isMajorOperation.value = true;
    error.value = null;
    if (amendCommit.value) {
      await gitService.amendCommit(commitMessage.value);
      toast.success("Commit amended successfully!", { title: "Success" });
      amendCommit.value = false;
    } else {
      await gitService.createCommit(commitMessage.value, stagedFiles.value);
      toast.success("Commit created successfully!", { title: "Success" });
    }
    commitMessage.value = "";
    selectedFile.value = null;
    await refreshRepo();
    error.value = null;
  } catch (err) { error.value = errMsg(err); }
  finally { loading.value = false; loadingMessage.value = ""; isMajorOperation.value = false; }
};

const handlePush = async () => {
  if (loading.value) return;
  try {
    loading.value = true;
    loadingMessage.value = "Pushing changes to remote...";
    isMajorOperation.value = true;
    error.value = null;
    await gitService.push();
    toast.success("Pushed successfully!", { title: "Success" });
    await refreshRepo();
    error.value = null;
  } catch (err) { error.value = errMsg(err); }
  finally { loading.value = false; loadingMessage.value = ""; isMajorOperation.value = false; }
};

const handlePull = async () => {
  if (loading.value) return;
  try {
    loading.value = true;
    loadingMessage.value = "Pulling from remote...";
    isMajorOperation.value = true;
    error.value = null;
    await gitService.pull();
    toast.success("Pulled successfully!", { title: "Success" });
    await refreshRepo();
    error.value = null;
  } catch (err) { error.value = errMsg(err); }
  finally { loading.value = false; loadingMessage.value = ""; isMajorOperation.value = false; }
};

const handleFetch = async () => {
  if (loading.value) return;
  try {
    loading.value = true;
    loadingMessage.value = "Fetching from remote...";
    isMajorOperation.value = false;
    error.value = null;
    await gitService.fetch();
    toast.success("Fetch completed!", { title: "Success" });
    await refreshRepo();
    error.value = null;
  } catch (err) { error.value = errMsg(err); }
  finally { loading.value = false; loadingMessage.value = ""; isMajorOperation.value = false; }
};

const handleOpenRepo = async (path?: string) => {
  if (loading.value) return;
  try {
    loading.value = true;
    loadingMessage.value = "Opening repository...";
    isMajorOperation.value = true;
    error.value = null;

    let selectedPath = path;
    if (!selectedPath) {
      const selected = await open({ directory: true, multiple: false, title: "Open Repository" });
      if (selected && typeof selected === "string") selectedPath = selected;
    }
    if (selectedPath) {
      const info = await gitService.openRepository(selectedPath);
      repoInfo.value = info;
      fetchSettings();
      selectedFile.value = null;
      selectedCommit.value = null;
    }
    error.value = null;
  } catch (err) { error.value = errMsg(err); }
  finally { loading.value = false; loadingMessage.value = ""; isMajorOperation.value = false; showRecentRepos.value = false; }
};

const triggerCloneModal = () => { cloneUrl.value = ""; clonePath.value = ""; showCloneModal.value = true; };

const handleBrowseClonePath = async () => {
  try {
    const selected = await open({ directory: true, multiple: false, title: "Select Clone Destination" });
    if (selected && typeof selected === "string") clonePath.value = selected;
  } catch (err) { error.value = errMsg(err); }
};

const handleCloneRepo = async () => {
  if (loading.value || !cloneUrl.value || !clonePath.value) return;
  const url = cloneUrl.value; const path = clonePath.value;
  showCloneModal.value = false;
  try {
    loading.value = true;
    error.value = null;
    await gitService.cloneRepository(url, path);
    const info = await gitService.openRepository(path);
    repoInfo.value = info;
    fetchSettings();
    selectedFile.value = null;
    error.value = null;
  } catch (err) { error.value = errMsg(err); }
  finally { loading.value = false; loadingMessage.value = ""; isMajorOperation.value = false; }
};

const handleDiscardChanges = async (path: string) => {
  if (loading.value) return;
  const confirmed = await ask(`Are you sure you want to discard changes in ${path}? This cannot be undone.`, { title: 'Discard Changes', kind: 'warning' });
  if (!confirmed) return;
  try {
    loading.value = true;
    loadingMessage.value = "Discarding changes...";
    await gitService.discardChanges(path);
    if (selectedFile.value === path) selectedFile.value = null;
    await refreshRepo();
    error.value = null;
  } catch (err) { error.value = errMsg(err); }
  finally { loading.value = false; loadingMessage.value = ""; }
};

const handleDiscardAllChanges = async () => {
  if (loading.value) return;
  const confirmed = await ask("Are you sure you want to discard ALL changes? This cannot be undone.", { title: 'Discard All Changes', kind: 'warning' });
  if (!confirmed) return;
  try {
    loading.value = true;
    loadingMessage.value = "Discarding all changes...";
    await gitService.discardAllChanges();
    selectedFile.value = null;
    await refreshRepo();
    error.value = null;
  } catch (err) { error.value = errMsg(err); }
  finally { loading.value = false; loadingMessage.value = ""; }
};

const handleCherryPick = async (sha: string) => {
  if (loading.value) return;
  const confirmed = await ask(`Cherry-pick commit ${sha.substring(0, 7)}?`, { title: 'Cherry-pick', kind: 'info' });
  if (!confirmed) return;
  try {
    loading.value = true;
    loadingMessage.value = "Cherry-picking commit...";
    isMajorOperation.value = true;
    await gitService.cherryPick(sha);
    await refreshRepo();
    toast.success("Cherry-pick successful", { title: "Success" });
    error.value = null;
  } catch (err) { error.value = errMsg(err); }
  finally { loading.value = false; loadingMessage.value = ""; isMajorOperation.value = false; }
};

const handleRevertCommit = async (sha: string) => {
  if (loading.value) return;
  const confirmed = await ask(`Revert commit ${sha.substring(0, 7)}?`, { title: 'Revert Commit', kind: 'warning' });
  if (!confirmed) return;
  try {
    loading.value = true;
    loadingMessage.value = "Reverting commit...";
    isMajorOperation.value = true;
    await gitService.revertCommit(sha);
    await refreshRepo();
    toast.success("Revert successful", { title: "Success" });
    error.value = null;
  } catch (err) { error.value = errMsg(err); }
  finally { loading.value = false; loadingMessage.value = ""; isMajorOperation.value = false; }
};

const handleStashSave = async () => {
  if (loading.value) return;
  const message = prompt("Optional stash message:");
  try {
    loading.value = true;
    loadingMessage.value = "Saving stash...";
    await gitService.stashSave(message || undefined);
    selectedFile.value = null;
    await refreshRepo();
    error.value = null;
  } catch (err) { error.value = errMsg(err); }
  finally { loading.value = false; loadingMessage.value = ""; }
};

const handleStashPop = async (index: number) => {
  if (loading.value) return;
  try {
    loading.value = true;
    loadingMessage.value = "Popping stash...";
    isMajorOperation.value = true;
    error.value = null;
    await gitService.stashPop(index);
    await refreshRepo();
    error.value = null;
  } catch (err) { error.value = errMsg(err); }
  finally { loading.value = false; loadingMessage.value = ""; isMajorOperation.value = false; }
};

const handleApplyStash = async (index: number) => {
  if (loading.value) return;
  try {
    loading.value = true;
    loadingMessage.value = "Applying stash...";
    isMajorOperation.value = true;
    error.value = null;
    await gitService.applyStash(index);
    await refreshRepo();
    toast.success("Stash applied successfully", { title: 'Success' });
    error.value = null;
  } catch (err) { error.value = errMsg(err); }
  finally { loading.value = false; loadingMessage.value = ""; isMajorOperation.value = false; }
};

const handleDropStash = async (index: number) => {
  if (loading.value) return;
  const confirmed = await ask("Are you sure you want to drop this stash? This cannot be undone.", { title: 'Drop Stash', kind: 'warning' });
  if (!confirmed) return;
  try {
    loading.value = true;
    loadingMessage.value = "Dropping stash...";
    error.value = null;
    await gitService.dropStash(index);
    await refreshRepo();
    toast.success("Stash dropped successfully", { title: 'Success' });
    error.value = null;
  } catch (err) { error.value = errMsg(err); }
  finally { loading.value = false; loadingMessage.value = ""; }
};

const handleBranchFromStash = async (index: number) => {
  if (loading.value) return;
  const branchName = prompt("Enter new branch name:");
  if (!branchName?.trim()) return;
  try {
    loading.value = true;
    loadingMessage.value = "Creating branch from stash...";
    isMajorOperation.value = true;
    error.value = null;
    await gitService.branchFromStash(index, branchName.trim());
    await refreshRepo();
    toast.success(`Branch ${branchName} created from stash`, { title: 'Success' });
    error.value = null;
  } catch (err) { error.value = errMsg(err); }
  finally { loading.value = false; loadingMessage.value = ""; isMajorOperation.value = false; }
};

const handleResolve = async (path: string, ours: boolean) => {
  if (loading.value) return;
  try {
    loading.value = true;
    loadingMessage.value = "Resolving conflict...";
    isMajorOperation.value = true;
    error.value = null;
    await gitService.resolveConflict(path, ours);
    await refreshRepo();
    error.value = null;
  } catch (err) { error.value = errMsg(err); }
  finally { loading.value = false; loadingMessage.value = ""; isMajorOperation.value = false; }
};

const checkoutBranch = async (branchName: string) => {
  if (loading.value) return;
  try {
    loading.value = true;
    loadingMessage.value = "Checking out branch...";
    isMajorOperation.value = true;
    error.value = null;
    await gitService.checkoutBranch(branchName);
    showBranchModal.value = false;
    selectedFile.value = null;
    selectedCommit.value = null;
    await refreshRepo();
    error.value = null;
  } catch (err) { error.value = errMsg(err); }
  finally { loading.value = false; loadingMessage.value = ""; isMajorOperation.value = false; }
};

const handleCreateBranch = async () => {
  if (loading.value || !newBranchName.value.trim()) return;
  try {
    loading.value = true;
    loadingMessage.value = "Creating branch...";
    isMajorOperation.value = true;
    await gitService.createBranch(newBranchName.value.trim());
    newBranchName.value = "";
    showBranchModal.value = false;
    await refreshRepo();
    error.value = null;
  } catch (err) { error.value = errMsg(err); }
  finally { loading.value = false; loadingMessage.value = ""; isMajorOperation.value = false; }
};

const handleSwitchToSSH = async () => {
  if (loading.value || !repoInfo.value) return;
  try {
    loading.value = true;
    loadingMessage.value = "Switching remote to SSH...";
    error.value = null;
    const currentUrl = await gitService.getRemoteUrl("origin");
    if (!currentUrl) { toast.error("No remote 'origin' found", { title: "Error" }); return; }
    if (currentUrl.startsWith("git@")) { toast.info("Remote is already using SSH protocol", { title: "Info" }); return; }
    if (!currentUrl.startsWith("https://")) { toast.error("Unsupported remote URL format", { title: "Error" }); return; }
    const urlMatch = currentUrl.match(/^https:\/\/([^\/]+)\/(.+)$/);
    if (!urlMatch) { toast.error("Could not parse remote URL", { title: "Error" }); return; }
    const host = urlMatch[1]; const path = urlMatch[2].replace(/\.git$/, '');
    const sshUrl = `git@${host}:${path}.git`;
    const confirmed = await ask(`Switch remote protocol to SSH?\nNew URL: ${sshUrl}`, { title: 'Switch Remote', kind: 'warning' });
    if (confirmed) {
      await gitService.setRemoteUrl("origin", sshUrl);
      toast.success("Remote protocol switched to SSH successfully!", { title: "Success" });
      showSettingsModal.value = false;
    }
    error.value = null;
  } catch (err) { error.value = errMsg(err); }
  finally { loading.value = false; loadingMessage.value = ""; }
};

const saveSettings = async () => {
  if (settings.value) {
    await gitService.saveSettings(settings.value);
    showSettingsModal.value = false;
  }
};

const toggleTheme = async () => {
  if (settings.value) {
    settings.value.theme = settings.value.theme === 'dark' ? 'light' : 'dark';
    document.documentElement.setAttribute('data-theme', settings.value.theme);
    try { await saveSettings(); } catch (err) { error.value = errMsg(err); }
  }
};

watch(() => settings.value?.theme, (newTheme) => {
  if (newTheme) document.documentElement.setAttribute('data-theme', newTheme);
}, { immediate: true });

const handleClickOutside = (event: MouseEvent) => {
  if (showRecentRepos.value && dropdownRef.value && !dropdownRef.value.contains(event.target as Node)) {
    showRecentRepos.value = false;
  }
};

const loadTags = async () => {
  try { tags.value = await gitService.listTags(); }
  catch (err) { console.error("Failed to load tags", err); error.value = errMsg(err); }
};

const loadRemotes = async () => {
  try { remotes.value = await gitService.listRemotes(); }
  catch (err) { console.error("Failed to load remotes", err); error.value = errMsg(err); }
};

const handleDeleteTag = async (name: string) => {
  if (loading.value) return;
  const confirmed = await ask(`Delete tag "${name}"?`, { title: 'Delete Tag', kind: 'warning' });
  if (!confirmed) return;
  try {
    loading.value = true;
    loadingMessage.value = "Deleting tag...";
    await gitService.deleteTag(name);
    await loadTags();
    toast.success(`Tag "${name}" deleted`, { title: 'Success' });
    error.value = null;
  } catch (err) { error.value = errMsg(err); }
  finally { loading.value = false; loadingMessage.value = ""; }
};

const handleAddRemote = async () => {
  if (loading.value || !newRemoteName.value.trim() || !newRemoteUrl.value.trim()) return;
  const name = newRemoteName.value.trim(); const url = newRemoteUrl.value.trim();
  try {
    loading.value = true;
    loadingMessage.value = "Adding remote...";
    await gitService.addRemote(name, url);
    newRemoteName.value = ""; newRemoteUrl.value = "";
    await loadRemotes();
    toast.success(`Remote "${name}" added`, { title: 'Success' });
    error.value = null;
  } catch (err) { error.value = errMsg(err); }
  finally { loading.value = false; loadingMessage.value = ""; }
};

const handleRemoveRemote = async (name: string) => {
  if (loading.value) return;
  const confirmed = await ask(`Remove remote "${name}"?`, { title: 'Remove Remote', kind: 'warning' });
  if (!confirmed) return;
  try {
    loading.value = true;
    loadingMessage.value = "Removing remote...";
    await gitService.removeRemote(name);
    await loadRemotes();
    toast.success(`Remote "${name}" removed`, { title: 'Success' });
    error.value = null;
  } catch (err) { error.value = errMsg(err); }
  finally { loading.value = false; loadingMessage.value = ""; }
};

const openSettingsModal = async () => {
  if (!settings.value) await fetchSettings();
  showSettingsModal.value = true;
};

useKeyboardShortcuts([
  { key: 's', ctrl: true, action: () => view.value === 'changes' && handleCommit(), description: 'Commit staged changes' },
  { key: 'p', ctrl: true, action: () => repoInfo.value && handlePush(), description: 'Push changes' },
  { key: 'P', ctrl: true, action: () => repoInfo.value && handlePull(), description: 'Pull changes' },
  { key: 'f', ctrl: true, action: () => repoInfo.value && handleFetch(), description: 'Fetch from remote' },
  { key: 'b', ctrl: true, action: () => repoInfo.value && (showBranchModal.value = true), description: 'Open branch switcher' },
  { key: 'k', ctrl: true, action: () => repoInfo.value && handleStashSave(), description: 'Stash changes' },
  { key: 'Escape', action: () => { showCloneModal.value = false; showSettingsModal.value = false; showBranchModal.value = false; showTagsModal.value = false; showRemotesModal.value = false; }, description: 'Close modal' },
]);
</script>

<template>
  <Toast />
  <ContextMenu :visible="isVisible" :position="position" :items="menuItems" @close="hideContextMenu" />
  <div class="app flex flex-col h-screen bg-background text-foreground overflow-hidden font-sans">
    <!-- Header/Top Bar -->
    <header class="h-12 border-b border-border flex items-center px-4 justify-between flex-shrink-0 relative top-accent-border" style="background: var(--header-bg);">
      <div class="flex items-center gap-6 text-sm">
        <div ref="dropdownRef" class="relative">
          <div class="flex items-center gap-2 cursor-pointer px-2.5 py-1.5 rounded-lg transition-safe hover:bg-muted" :class="{ 'bg-muted': showRecentRepos }" @click="showRecentRepos = !showRecentRepos" style="color: var(--foreground);">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="color: var(--mark); flex-shrink:0;">
              <path d="M12 2L2 7l10 5 10-5-10-5z"/><path d="M2 17l10 5 10-5"/><path d="M2 12l10 5 10-5"/>
            </svg>
            <div class="flex items-center gap-1.5">
              <span class="text-[11px] font-medium" style="color: var(--muted-foreground);">repo</span>
              <span class="font-semibold text-[13px]" style="color: var(--foreground);">{{ repoInfo ? currentProjectName : 'None' }}</span>
            </div>
            <div v-if="repoInfo && (repoInfo.ahead > 0 || repoInfo.behind > 0)" class="flex items-center gap-1 ml-1">
              <span v-if="repoInfo.ahead > 0" class="badge text-[10px] font-bold" style="background: var(--success-bg); color: var(--success);">↑{{ repoInfo.ahead }}</span>
              <span v-if="repoInfo.behind > 0" class="badge text-[10px] font-bold" style="background: var(--error-bg); color: var(--error);">↓{{ repoInfo.behind }}</span>
            </div>
            <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" class="text-muted-foreground transition-transform duration-200" :class="{ 'rotate-180': showRecentRepos }"><polyline points="6 9 12 15 18 9"/></svg>
          </div>
          <div v-if="showRecentRepos" class="absolute top-full left-0 mt-2 w-80 rounded-xl z-50 overflow-hidden py-1.5 glass shadow-xl" :style="{ border: '1px solid var(--border)', animation: 'slide-down 0.2s cubic-bezier(0.16,1,0.3,1)' }">
            <div class="px-4 py-2 border-b flex justify-between items-center" style="border-color: var(--border);">
              <span class="text-[10px] font-bold uppercase tracking-widest" style="color: var(--muted-foreground);">Recent Repositories</span>
              <button @click="handleOpenRepo()" class="text-[10px] font-bold transition-safe hover:underline" style="color: var(--accent);">OPEN NEW</button>
            </div>
            <div class="max-h-64 overflow-y-auto">
              <div v-for="path in settings?.recent_repositories" :key="path" @click="handleOpenRepo(path)" class="px-4 py-2.5 cursor-pointer transition-safe flex flex-col gap-0.5 hover:bg-muted" :class="{ 'bg-spotlight': repoInfo?.path === path }">
                <div class="text-[13px] font-semibold truncate flex items-center justify-between gap-2">
                  <div class="flex items-center gap-2 truncate">
                    <span v-if="repoInfo?.path === path" class="w-1.5 h-1.5 rounded-full flex-shrink-0" style="background: var(--accent);"></span>
                    {{ getRepoName(path) }}
                  </div>
                  <div v-if="getRecentRepoInfo(path)" class="flex items-center gap-1 flex-shrink-0">
                    <span v-if="getRecentRepoInfo(path)?.ahead" class="badge text-[9px] font-bold" style="background:var(--success-bg);color:var(--success);">↑{{ getRecentRepoInfo(path)?.ahead }}</span>
                    <span v-if="getRecentRepoInfo(path)?.behind" class="badge text-[9px] font-bold" style="background:var(--error-bg);color:var(--error);">↓{{ getRecentRepoInfo(path)?.behind }}</span>
                    <span v-if="getRecentRepoInfo(path)?.is_dirty" class="w-1.5 h-1.5 rounded-full" style="background:var(--warning);"></span>
                  </div>
                </div>
                <div class="text-[10px] font-mono truncate" style="color: var(--muted-foreground);">{{ path }}</div>
              </div>
              <div v-if="!settings?.recent_repositories.length" class="px-4 py-4 text-center text-xs italic" style="color: var(--muted-foreground);">No recent repositories</div>
            </div>
          </div>
        </div>

        <div v-if="repoInfo" class="flex items-center gap-2 cursor-pointer px-2.5 py-1.5 rounded-lg transition-safe hover:bg-muted" @click="showBranchModal = true" style="color: var(--foreground);">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="color: var(--muted-foreground);"><line x1="6" y1="3" x2="6" y2="15"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M18 9a9 9 0 0 1-9 9"/></svg>
          <span class="font-medium text-[13px]" style="color: var(--foreground);">{{ getCurrentBranch() }}</span>
        </div>
      </div>

      <div v-if="repoInfo" class="absolute left-1/2 -translate-x-1/2 hidden md:flex items-center gap-2 pointer-events-none">
        <span class="text-[11px] font-semibold uppercase tracking-widest" style="color: var(--muted-foreground);">{{ currentProjectName }}</span>
      </div>

      <div class="flex items-center gap-1.5">
        <button v-if="repoInfo" @click="triggerCloneModal" class="btn btn-ghost h-8 px-3 text-[13px]">Clone</button>
        <button v-if="repoInfo" @click="handleFetch" class="btn btn-ghost h-8 px-3 text-[13px]">Fetch</button>
        <button v-if="repoInfo" @click="() => { loadTags(); showTagsModal = true; }" class="btn btn-ghost h-8 px-3 text-[13px]">Tags</button>
        <button v-if="repoInfo" @click="() => { loadRemotes(); showRemotesModal = true; }" class="btn btn-ghost h-8 px-3 text-[13px]">Remotes</button>
        <div class="w-px h-5 mx-1" style="background: var(--border);"></div>
        <button @click="openSettingsModal" class="btn btn-ghost h-8 w-8 p-0" title="Settings" style="color: var(--foreground);">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
        </button>
        <button @click="toggleTheme" class="btn btn-ghost h-8 w-8 p-0" :title="settings?.theme === 'dark' ? 'Light Mode' : 'Dark Mode'" style="color: var(--foreground);">
          <svg v-if="settings?.theme === 'dark'" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="5"/><line x1="12" y1="1" x2="12" y2="3"/><line x1="12" y1="21" x2="12" y2="23"/><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/><line x1="1" y1="12" x2="3" y2="12"/><line x1="21" y1="12" x2="23" y2="12"/><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/></svg>
          <svg v-else width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/></svg>
        </button>
      </div>
    </header>

    <!-- Error Banner -->
    <div v-if="error" class="border-b px-4 py-2.5 text-[13px] flex justify-between items-center" style="background: var(--error-bg); border-color: rgba(248,81,73,0.2); color: var(--error);">
      <span class="font-medium truncate mr-3">{{ error }}</span>
      <button @click="clearError" class="btn btn-ghost text-xs px-2 py-1">✕</button>
    </div>

    <!-- Modals -->
    <div v-if="showCloneModal || showSettingsModal || showBranchModal || showTagsModal || showRemotesModal" class="fixed inset-0 flex items-center justify-center z-[100] p-4" style="background: rgba(0,0,0,0.65); backdrop-filter: blur(8px);">
      <div v-if="showCloneModal" class="bg-card rounded-2xl shadow-xl p-8 w-full max-w-md border border-border">
        <h2 class="text-2xl font-display mb-6 text-foreground">Clone Repository</h2>
        <div class="mb-5">
          <label class="block mb-2 text-sm font-medium text-foreground">Remote URL</label>
          <input v-model="cloneUrl" placeholder="https://github.com/user/repo.git" class="w-full border border-border rounded-lg p-3 text-foreground text-sm focus:ring-2 focus:ring-accent focus:border-transparent outline-none" />
        </div>
        <div class="mb-8">
          <label class="block mb-2 text-sm font-medium text-foreground">Destination Path</label>
          <div class="flex gap-2">
            <input v-model="clonePath" placeholder="/path/to/destination" class="flex-1 border border-border rounded-lg p-3 text-foreground text-sm focus:ring-2 focus:ring-accent focus:border-transparent outline-none" />
            <button @click="handleBrowseClonePath" class="px-4 py-3 border border-border rounded-lg hover:bg-muted transition-safe text-sm font-medium">Browse</button>
          </div>
        </div>
        <div class="flex justify-end gap-3">
          <button @click="showCloneModal = false" class="px-6 py-2.5 border border-border rounded-lg hover:bg-muted transition-safe font-medium">Cancel</button>
          <button @click="handleCloneRepo" :disabled="!cloneUrl || !clonePath" class="gradient-bg text-accent-foreground px-6 py-2.5 rounded-lg disabled:opacity-50 disabled:cursor-not-allowed hover:shadow-accent transition-safe font-semibold">Clone</button>
        </div>
      </div>

      <div v-if="showSettingsModal" class="bg-card rounded-2xl shadow-xl p-8 w-full max-w-md border border-border">
        <h2 class="text-2xl font-display mb-6 text-foreground">Settings</h2>
        <div v-if="settings" class="space-y-5 mb-8">
          <div>
            <label class="block text-sm font-semibold text-foreground mb-1">Git User Name</label>
            <p class="text-[11px] text-muted-foreground mb-2 leading-tight">Identifies you as the author of commits</p>
            <input v-model="settings.user_name" class="w-full border border-border rounded-lg p-3 text-foreground text-sm outline-none focus:ring-2 focus:ring-accent focus:border-transparent bg-white shadow-sm" />
          </div>
          <div>
            <label class="block text-sm font-semibold text-foreground mb-1">Git User Email</label>
            <p class="text-[11px] text-muted-foreground mb-2 leading-tight">Email address associated with your commits</p>
            <input v-model="settings.user_email" class="w-full border border-border rounded-lg p-3 text-foreground text-sm outline-none focus:ring-2 focus:ring-accent focus:border-transparent bg-white shadow-sm" />
          </div>
          <div>
            <label class="block text-sm font-semibold text-foreground mb-1">SSH Key Path</label>
            <input v-model="settings.ssh_key_path" placeholder="~/.ssh/id_rsa" class="w-full border border-border rounded-lg p-3 text-foreground text-sm outline-none focus:ring-2 focus:ring-accent focus:border-transparent font-mono bg-white shadow-sm" />
          </div>
          <div class="pt-4 border-t border-border">
            <button @click="handleSwitchToSSH" class="text-sm text-accent hover:underline font-semibold flex items-center gap-2"><span>⚠️</span> Switch remotes to SSH</button>
            <p class="text-[11px] text-muted-foreground mt-1 leading-tight">Use this if you get authentication errors with HTTPS</p>
          </div>
        </div>
        <div v-else class="space-y-5 mb-8 flex items-center justify-center py-8">
          <div class="spinner"></div>
          <span class="text-sm text-muted-foreground">Loading settings...</span>
        </div>
        <div v-if="settings" class="flex justify-end gap-3">
          <button @click="showSettingsModal = false" class="px-6 py-2.5 border border-border rounded-lg hover:bg-muted transition-safe font-medium">Cancel</button>
          <button @click="saveSettings" class="gradient-bg text-accent-foreground px-6 py-2.5 rounded-lg hover:shadow-accent transition-safe font-semibold">Save</button>
        </div>
      </div>

      <div v-if="showBranchModal" class="bg-card rounded-2xl shadow-xl p-8 w-full max-w-md border border-border">
        <h2 class="text-2xl font-display mb-6 text-foreground">Branches</h2>
        <div class="max-h-60 overflow-auto mb-6 space-y-2">
          <div v-for="branch in branches" :key="branch.name" @click="!branch.is_current && checkoutBranch(branch.name)" class="p-3 rounded-lg border border-transparent hover:border-border cursor-pointer flex items-center justify-between text-sm transition-safe" :class="{ 'gradient-bg text-accent-foreground border-accent shadow-accent': branch.is_current, 'hover:bg-muted': !branch.is_current }">
            <span class="font-medium">{{ branch.name }}</span>
            <span v-if="branch.is_current" class="text-xs font-semibold">Active</span>
          </div>
        </div>
        <div class="border-t border-border pt-6">
          <label class="block text-sm font-medium text-foreground mb-2">Create New Branch</label>
          <div class="flex gap-2">
            <input v-model="newBranchName" @keyup.enter="handleCreateBranch" placeholder="feature/new-branch" class="flex-1 border border-border rounded-lg p-3 text-foreground text-sm outline-none focus:ring-2 focus:ring-accent focus:border-transparent font-mono" />
            <button @click="handleCreateBranch" class="gradient-bg text-accent-foreground px-5 rounded-lg hover:shadow-accent transition-safe font-semibold">+</button>
          </div>
        </div>
        <div class="flex justify-end mt-6"><button @click="showBranchModal = false" class="px-6 py-2.5 border border-border rounded-lg hover:bg-muted transition-safe font-medium">Close</button></div>
      </div>

      <div v-if="showTagsModal" class="bg-card rounded-2xl shadow-xl p-8 w-full max-w-md border border-border">
        <h2 class="text-2xl font-display mb-6 text-foreground">Tags</h2>
        <div class="max-h-72 overflow-auto mb-6 space-y-2">
          <div v-if="tags.length === 0" class="text-center text-muted-foreground text-sm py-8">No tags found in this repository</div>
          <div v-for="tag in tags" :key="tag.name" class="p-3 rounded-lg border border-border hover:border-accent cursor-pointer flex items-center justify-between group transition-safe">
            <div class="flex-1 min-w-0">
              <div class="text-sm font-semibold truncate flex items-center gap-2"><span class="text-accent">⚮</span> {{ tag.name }}</div>
              <div class="text-xs text-muted-foreground font-mono mt-1">{{ tag.sha.substring(0, 7) }}<span v-if="tag.date" class="ml-2">{{ new Date(tag.date * 1000).toLocaleDateString() }}</span></div>
              <div v-if="tag.message" class="text-[10px] text-muted-foreground mt-1 truncate">{{ tag.message }}</div>
            </div>
            <button @click="handleDeleteTag(tag.name)" class="opacity-0 group-hover:opacity-100 text-error hover:bg-error/10 px-3 py-1.5 rounded text-xs transition-opacity font-medium">Delete</button>
          </div>
        </div>
        <div class="flex justify-end mt-6"><button @click="showTagsModal = false" class="px-6 py-2.5 border border-border rounded-lg hover:bg-muted transition-safe font-medium">Close</button></div>
      </div>

      <div v-if="showRemotesModal" class="bg-card rounded-2xl shadow-xl p-8 w-full max-w-md border border-border">
        <h2 class="text-2xl font-display mb-6 text-foreground">Remotes</h2>
        <div class="max-h-72 overflow-auto mb-6 space-y-2">
          <div v-if="remotes.length === 0" class="text-center text-muted-foreground text-sm py-8">No remotes configured</div>
          <div v-for="remote in remotes" :key="remote.name" class="p-3 rounded-lg border border-border hover:border-accent flex items-center justify-between group transition-safe">
            <div class="flex-1 min-w-0 mr-3">
              <div class="text-sm font-semibold flex items-center gap-2"><span class="text-accent">☁</span> {{ remote.name }}</div>
              <div class="text-xs text-muted-foreground font-mono mt-1 truncate" :title="remote.url">{{ remote.url }}</div>
            </div>
            <button @click="handleRemoveRemote(remote.name)" class="opacity-0 group-hover:opacity-100 text-error hover:bg-error/10 px-3 py-1.5 rounded text-xs transition-opacity font-medium">Remove</button>
          </div>
        </div>
        <div class="border-t border-border pt-6">
          <label class="block text-sm font-medium text-foreground mb-2">Add Remote</label>
          <div class="space-y-3">
            <input v-model="newRemoteName" placeholder="e.g. upstream" class="w-full border border-border rounded-lg p-3 text-foreground text-sm outline-none focus:ring-2 focus:ring-accent focus:border-transparent" />
            <input v-model="newRemoteUrl" placeholder="https://github.com/user/repo.git" class="w-full border border-border rounded-lg p-3 text-foreground text-sm outline-none focus:ring-2 focus:ring-accent focus:border-transparent font-mono" />
            <button @click="handleAddRemote" :disabled="!newRemoteName.trim() || !newRemoteUrl.trim()" class="w-full gradient-bg text-accent-foreground disabled:opacity-50 disabled:cursor-not-allowed px-5 py-2.5 rounded-lg hover:shadow-accent transition-safe font-semibold">Add Remote</button>
          </div>
        </div>
        <div class="flex justify-end mt-6"><button @click="showRemotesModal = false" class="px-6 py-2.5 border border-border rounded-lg hover:bg-muted transition-safe font-medium">Close</button></div>
      </div>
    </div>

    <!-- Main Content Area -->
    <div v-if="repoInfo" class="flex flex-1 overflow-hidden">
      <aside class="w-80 min-w-[20rem] max-w-[20rem] flex-shrink-0 border-r border-border flex flex-col bg-card shadow-sm">
        <div class="flex border-b text-[12px] font-semibold" style="border-color: var(--border);">
          <button @click="view = 'changes'" class="flex-1 py-2.5 transition-safe relative border-r" style="border-color: var(--border);" :style="view === 'changes' ? 'color: var(--accent);' : 'color: var(--muted-foreground);'">
            Changes<span v-if="fileStatuses.length > 0" class="ml-1 badge" style="background:var(--spotlight);color:var(--accent);">{{ fileStatuses.length }}</span>
            <span v-if="view === 'changes'" class="absolute bottom-0 left-0 right-0 h-0.5 gradient-bg"></span>
          </button>
          <button @click="view = 'history'" class="flex-1 py-2.5 transition-safe relative" :class="{ 'border-r': stashes.length > 0 || conflicts.length > 0 }" style="border-color: var(--border);" :style="view === 'history' ? 'color: var(--accent);' : 'color: var(--muted-foreground);'">
            History<span v-if="view === 'history'" class="absolute bottom-0 left-0 right-0 h-0.5 gradient-bg"></span>
          </button>
          <button v-if="stashes.length > 0" @click="view = 'stashes'" class="flex-1 py-2.5 transition-safe relative border-r" style="border-color: var(--border);" :style="view === 'stashes' ? 'color: var(--accent);' : 'color: var(--muted-foreground);'">
            Stash<span v-if="view === 'stashes'" class="absolute bottom-0 left-0 right-0 h-0.5 gradient-bg"></span>
          </button>
          <button v-if="conflicts.length > 0" @click="view = 'conflicts'" class="flex-1 py-2.5 transition-safe relative" style="color: var(--error);">
            Conflicts<span v-if="view === 'conflicts'" class="absolute bottom-0 left-0 right-0 h-0.5" style="background: var(--error);"></span>
          </button>
        </div>

        <div class="flex-1 overflow-auto p-3">
          <div v-if="view === 'changes'" class="space-y-1.5">
            <div v-if="fileStatuses.length > 0" @contextmenu.prevent="onFileHeaderContextMenu" class="flex items-center gap-3 p-2.5 mb-2 rounded-lg bg-muted/50 border border-border transition-safe justify-between">
              <div class="flex items-center gap-3 cursor-pointer" @click="toggleAllStaged">
                <input type="checkbox" :checked="allStaged" class="w-4 h-4 rounded border-border accent-accent cursor-pointer pointer-events-none" />
                <div class="text-xs font-semibold text-muted-foreground select-none">{{ fileStatuses.length }} changed file{{ fileStatuses.length !== 1 ? 's' : '' }}</div>
              </div>
              <button @click.stop="handleDiscardAllChanges" class="text-[10px] text-error hover:underline font-bold px-2 py-1 rounded hover:bg-error/10 transition-safe">DISCARD ALL</button>
            </div>

            <div v-for="file in fileStatuses" :key="file.path" @contextmenu.prevent="onFileContextMenu($event, file)" class="group flex items-center gap-2.5 px-2.5 py-2 rounded-lg cursor-pointer transition-safe" :style="selectedFile === file.path ? 'background:var(--spotlight); border:1px solid var(--accent); border-opacity:0.3;' : 'border:1px solid transparent;'" :class="{ 'hover:bg-muted': selectedFile !== file.path }" @click.self="selectedFile = file.path">
              <input type="checkbox" :checked="file.staged" @change="toggleStaged(file)" class="w-3.5 h-3.5 rounded flex-shrink-0" style="accent-color: var(--accent);" />
              <div class="flex-1 min-w-0 flex items-center gap-2" @click="selectedFile = file.path">
                <span class="text-[10px] w-4 text-center font-bold flex-shrink-0" :style="file.status === 'added' ? 'color:var(--success)' : file.status === 'deleted' ? 'color:var(--error)' : 'color:var(--accent)'">{{ file.status[0].toUpperCase() }}</span>
                <span class="truncate text-[13px]" :title="file.path">{{ file.path.split('/').pop() }}</span>
                <span class="text-[10px] font-mono truncate flex-shrink-0" style="color:var(--muted-foreground); max-width:60px;">{{ file.path.includes('/') ? file.path.substring(0, file.path.lastIndexOf('/')) : '' }}</span>
              </div>
              <button @click.stop="handleDiscardChanges(file.path)" class="opacity-0 group-hover:opacity-100 w-5 h-5 flex items-center justify-center rounded transition-safe flex-shrink-0 text-[10px]" style="color:var(--error);" onmouseover="this.style.background='var(--error-bg)'" onmouseout="this.style.background=''">✕</button>
            </div>
          </div>

          <div v-else-if="view === 'history'" class="flex-1 flex flex-col overflow-hidden">
            <div class="px-3 py-2 border-b border-border bg-card/50">
              <input v-model="searchCommitQuery" placeholder="Search commits..." class="w-full bg-muted/30 border border-border rounded-lg px-3 py-2 text-xs text-foreground outline-none focus:ring-1 focus:ring-accent" />
              <span class="text-[10px] text-muted-foreground ml-2">{{ filteredCommits.length }} / {{ commits.length }}</span>
            </div>
            <RecycleScroller class="flex-1 overflow-auto p-3" :items="filteredCommits" :item-size="76" key-field="sha" v-slot="{ item }">
              <div @click="selectedCommit = item" @contextmenu.prevent="onCommitContextMenu($event, item)" class="mb-1.5 p-3 rounded-lg border border-transparent hover:border-border cursor-pointer transition-safe bg-card/30" :class="{ 'border-accent bg-accent/5 shadow-sm': selectedCommit?.sha === item.sha }">
                <div class="text-sm font-semibold truncate mb-1.5 flex items-center gap-2" :class="{ 'text-accent': selectedCommit?.sha === item.sha }">
                  <span v-if="!item.is_pushed" class="text-success font-bold text-xs" title="Unpushed commit">↑</span>
                  {{ item.message }}
                </div>
                <div class="flex justify-between text-xs text-muted-foreground font-mono">
                  <span>{{ item.sha.substring(0, 7) }}</span>
                  <span>{{ new Date(item.timestamp * 1000).toLocaleDateString() }}</span>
                </div>
              </div>
            </RecycleScroller>
          </div>

          <div v-else-if="view === 'stashes'" class="space-y-1.5">
            <div v-for="(stash, index) in stashes" :key="index" @contextmenu.prevent="onStashContextMenu($event, stash)" class="p-3 bg-card rounded-lg border border-border flex justify-between items-center group hover:border-accent transition-safe">
              <div class="flex-1 min-w-0">
                <div class="text-sm font-semibold truncate">{{ stash.message || 'No message' }}</div>
                <div class="text-xs text-muted-foreground font-mono mt-1">{{ stash.sha.substring(0, 7) }}</div>
              </div>
              <button @click="handleStashPop(index)" class="opacity-0 group-hover:opacity-100 gradient-bg text-accent-foreground text-xs px-3 py-1.5 rounded-lg hover:shadow-accent transition-safe font-medium">Pop</button>
            </div>
          </div>

          <div v-else-if="view === 'conflicts'" class="space-y-2">
            <div v-for="conflict in conflicts" :key="conflict.path" @contextmenu.prevent="onConflictContextMenu($event, conflict)" class="p-3 bg-error/5 rounded-lg border border-error/20">
              <div class="text-sm font-semibold truncate mb-3 text-error" :title="conflict.path">{{ conflict.path.split('/').pop() }}</div>
              <div class="flex gap-2 text-xs">
                <button @click="handleResolve(conflict.path, true)" class="flex-1 bg-card border border-border hover:bg-muted py-2 rounded-lg font-medium transition-safe">Use Ours</button>
                <button @click="handleResolve(conflict.path, false)" class="flex-1 bg-card border border-border hover:bg-muted py-2 rounded-lg font-medium transition-safe">Use Theirs</button>
              </div>
            </div>
          </div>
        </div>

        <div v-if="view === 'changes'" class="p-4 border-t border-border bg-muted/30">
          <div class="flex items-center gap-2 mb-2">
            <input type="checkbox" id="amend" v-model="amendCommit" class="w-3.5 h-3.5 rounded border-border accent-accent" />
            <label for="amend" class="text-xs font-medium text-muted-foreground cursor-pointer select-none">Amend Last Commit</label>
          </div>
          <label class="block mb-2 text-sm font-medium text-foreground">Commit Message</label>
          <textarea v-model="commitMessage" placeholder="Describe your changes..." class="w-full bg-card border border-border rounded-lg p-3 text-foreground text-sm mb-3 focus:ring-2 focus:ring-accent focus:border-transparent outline-none resize-none" rows="3" />
          <button @click="handleCommit" :disabled="loading || !commitMessage.trim() || (!amendCommit && stagedFiles.length === 0)" class="w-full gradient-bg text-accent-foreground disabled:opacity-50 disabled:cursor-not-allowed py-2.5 rounded-lg font-semibold text-sm hover:shadow-accent transition-safe">{{ amendCommit ? 'Amend Commit' : `Commit to ${getCurrentBranch()}` }}</button>
        </div>

        <div class="p-3 border-t border-border flex gap-2 overflow-x-auto bg-card text-sm">
          <button @click="handlePull" class="flex-1 bg-card border border-border py-2 px-3 rounded-lg hover:bg-muted transition-safe font-medium flex items-center justify-center gap-2">
            Pull<span v-if="repoInfo?.behind" class="flex items-center justify-center bg-error/10 text-error text-[10px] w-4 h-4 rounded-full font-bold">{{ repoInfo.behind }}</span>
          </button>
          <button @click="handlePush" class="flex-1 bg-card border border-border py-2 px-3 rounded-lg hover:bg-muted transition-safe font-medium flex items-center justify-center gap-2">
            Push<span v-if="repoInfo?.ahead" class="flex items-center justify-center bg-success/10 text-success text-[10px] w-4 h-4 rounded-full font-bold">{{ repoInfo.ahead }}</span>
          </button>
          <button @click="handleStashSave" class="flex-1 bg-card border border-border py-2 px-3 rounded-lg hover:bg-muted transition-safe font-medium">Stash</button>
          <button v-if="view === 'history' && selectedCommit" @click="selectedCommit = null" class="flex-1 bg-card border border-border py-2 px-3 rounded-lg hover:bg-error/10 hover:text-error transition-safe font-medium">Clear</button>
        </div>
      </aside>

      <main class="flex-1 bg-background flex flex-col overflow-hidden">
        <div v-if="view === 'changes' && selectedFile" class="flex-1 flex flex-col overflow-hidden">
          <div class="h-12 border-b border-border flex items-center px-6 bg-card text-sm font-mono text-muted-foreground">{{ selectedFile }}</div>
          <div class="flex-1 overflow-auto"><DiffViewer :diffs="diffs" /></div>
        </div>
        <div v-else-if="view === 'history' && selectedCommit" class="flex-1 flex flex-col overflow-hidden">
          <div class="h-14 border-b border-border flex items-center px-6 bg-card text-sm font-mono justify-between flex-shrink-0">
            <div class="flex items-center gap-3 overflow-hidden">
              <span class="text-accent font-semibold flex-shrink-0">{{ selectedCommit.sha.substring(0, 7) }}</span>
              <span class="text-muted-foreground truncate" :title="selectedCommit.message">{{ selectedCommit.message }}</span>
            </div>
            <div class="flex items-center gap-3 flex-shrink-0 ml-4">
              <button @click="handleCherryPick(selectedCommit.sha)" class="px-3 py-1.5 border border-border rounded text-xs hover:bg-muted transition-safe font-medium" title="Apply this commit to current branch">Cherry-pick</button>
              <button @click="handleRevertCommit(selectedCommit.sha)" class="px-3 py-1.5 border border-border rounded text-xs hover:bg-muted hover:text-error transition-safe font-medium" title="Create a new commit that reverts this one">Revert</button>
            </div>
          </div>
          <div class="flex-1 flex overflow-hidden">
            <div class="w-64 border-r border-border bg-card overflow-y-auto flex-shrink-0">
              <div v-for="diff in diffs" :key="diff.path" @click="selectedCommitFile = diff.path" @contextmenu.prevent="onCommitFileContextMenu($event, diff.path)" class="px-4 py-2 text-sm cursor-pointer border-l-2 hover:bg-muted transition-safe flex items-center justify-between group" :class="{ 'border-accent bg-accent/5': selectedCommitFile === diff.path, 'border-transparent': selectedCommitFile !== diff.path }">
                <span class="truncate" :title="diff.path">{{ diff.path.split('/').pop() }}</span>
                <span class="text-xs w-4 text-center font-bold" :class="{ 'text-success': diff.additions > 0 && diff.deletions === 0, 'text-error': diff.deletions > 0 && diff.additions === 0, 'text-accent': diff.additions > 0 && diff.deletions > 0 }">{{ diff.additions > 0 && diff.deletions === 0 ? 'A' : (diff.deletions > 0 && diff.additions === 0 ? 'D' : 'M') }}</span>
              </div>
            </div>
            <div class="flex-1 overflow-auto bg-background"><DiffViewer :diffs="diffs.filter(d => d.path === selectedCommitFile)" /></div>
          </div>
        </div>
        <div v-else class="flex-1 flex items-center justify-center text-muted-foreground text-sm">{{ view === 'history' ? 'Select a commit to view diff' : 'Select a file to view changes' }}</div>
      </main>
    </div>

    <!-- Welcome View -->
    <div v-else class="flex-1 flex flex-col items-center justify-center bg-background grid-pattern relative overflow-hidden">
      <div class="relative max-w-lg w-full text-center px-8" style="animation: fade-in-up 0.4s ease;">
        <div class="flex justify-center mb-7">
          <div class="relative">
            <div class="w-16 h-16 rounded-2xl flex items-center justify-center" style="background: var(--foreground);">
              <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" style="stroke: var(--accent-foreground);"><path d="M12 2L2 7l10 5 10-5-10-5z"/><path d="M2 17l10 5 10-5"/><path d="M2 12l10 5 10-5"/></svg>
            </div>
            <div class="absolute -top-1 -right-1 w-3.5 h-3.5 rounded-full border-2" style="background: var(--mark); border-color: var(--background);"></div>
          </div>
        </div>
        <h1 class="text-4xl font-semibold tracking-tight mb-2" style="color: var(--foreground); letter-spacing: -0.03em;">Ark</h1>
        <p class="text-[14px] mb-10" style="color: var(--muted-foreground);">A precision Git client for professional workflows</p>
        <div class="grid grid-cols-2 gap-3 mb-8">
          <button @click="handleOpenRepo()" class="group p-5 rounded-xl border text-left transition-safe" style="background: var(--card); border-color: var(--border);" onmouseover="this.style.background='var(--muted)'" onmouseout="this.style.background='var(--card)'">
            <div class="w-8 h-8 rounded-lg flex items-center justify-center mb-3" style="background: var(--muted-foreground); opacity:0.15;">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="stroke: var(--foreground); opacity:1;"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>
            </div>
            <div class="font-medium text-[14px] mb-0.5" style="color: var(--foreground);">Open</div>
            <div class="text-[12px]" style="color: var(--muted-foreground);">Load from filesystem</div>
          </button>
          <button @click="triggerCloneModal" class="group p-5 rounded-xl border text-left transition-safe" style="background: var(--card); border-color: var(--border);" onmouseover="this.style.background='var(--muted)'" onmouseout="this.style.background='var(--card)'">
            <div class="w-8 h-8 rounded-lg flex items-center justify-center mb-3" style="background: var(--muted);">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="stroke: var(--mark);"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
            </div>
            <div class="font-medium text-[14px] mb-0.5" style="color: var(--foreground);">Clone</div>
            <div class="text-[12px]" style="color: var(--muted-foreground);">From remote URL</div>
          </button>
        </div>

        <div v-if="settings?.recent_repositories.length" class="text-left">
          <div class="flex items-center gap-3 mb-3">
            <div class="h-px flex-1" style="background: var(--border);"></div>
            <span class="text-[11px] font-semibold uppercase tracking-widest" style="color: var(--muted-foreground);">Recent</span>
            <div class="h-px flex-1" style="background: var(--border);"></div>
          </div>
          <div class="space-y-1.5">
            <div v-for="path in settings.recent_repositories.slice(0, 5)" :key="path" @click="handleOpenRepo(path)" class="group flex items-center gap-3 px-4 py-3 rounded-xl border cursor-pointer transition-safe" style="background: var(--card); border-color: var(--border);" onmouseover="this.style.borderColor='var(--accent)'" onmouseout="this.style.borderColor='var(--border)'">
              <div class="w-7 h-7 rounded-lg flex items-center justify-center flex-shrink-0 text-[11px] font-bold" style="background:var(--muted); color:var(--accent);">{{ getRepoName(path)[0]?.toUpperCase() }}</div>
              <div class="flex-1 min-w-0">
                <div class="font-semibold truncate text-[13px]">{{ getRepoName(path) }}</div>
                <div class="text-[11px] font-mono truncate" style="color:var(--muted-foreground);">{{ path }}</div>
              </div>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="flex-shrink-0 transition-safe" style="color:var(--muted-foreground);" onmouseover="this.style.color='var(--accent)'"><polyline points="9 18 15 12 9 6"/></svg>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Loading Overlay -->
    <div v-if="loading && isMajorOperation" class="fixed inset-0 z-[100] flex items-center justify-center" style="background: rgba(0,0,0,0.6); backdrop-filter: blur(8px);">
      <div class="rounded-2xl p-8 flex flex-col items-center gap-5" style="background:var(--card); border:1px solid var(--border); box-shadow:var(--shadow-xl);">
        <div class="spinner spinner-lg"></div>
        <span class="text-[15px] font-semibold">{{ loadingMessage || 'Processing...' }}</span>
      </div>
    </div>
    <div v-else-if="loading" class="fixed bottom-4 right-4 z-[100] flex items-center gap-2 px-3 py-2 rounded-xl" style="background:var(--card); border:1px solid var(--border); box-shadow:var(--shadow-lg); backdrop-filter:blur(12px);">
      <div class="spinner"></div>
      <span class="text-[12px] font-medium" style="color:var(--muted-foreground);">{{ loadingMessage || 'Loading...' }}</span>
    </div>
  </div>
</template>

<style>
/* Minimalist modern styles defined in index.css */
</style>
