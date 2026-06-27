<script setup lang="ts">
import { ref, reactive, onMounted, onUnmounted, watch, onErrorCaptured } from 'vue';
import { gitService, type CommitInfo, type StageResult } from './services/git';
import { open, ask } from '@tauri-apps/plugin-dialog';
import { useToast } from './composables/useToast';
import Toast from './components/Toast.vue';
import { useContextMenu } from './composables/useContextMenu';
import ContextMenu from './components/ContextMenu.vue';
import { useKeyboardShortcuts } from './composables/useKeyboardShortcuts';
import { listen } from '@tauri-apps/api/event';

// Import stores
import { useRepoStore } from './stores/repo';
import { useUIStore } from './stores/ui';
import { useSettingsStore } from './stores/settings';

// Import components
import HeaderBar from './components/HeaderBar.vue';
import ChangesPanel from './components/ChangesPanel.vue';
import CommitPanel from './components/CommitPanel.vue';
import HistoryPanel from './components/HistoryPanel.vue';
import DiffPanel from './components/DiffPanel.vue';
import StashPanel from './components/StashPanel.vue';
import ConflictsPanel from './components/ConflictsPanel.vue';
import CloneModal from './components/CloneModal.vue';
import SettingsModal from './components/SettingsModal.vue';
import BranchModal from './components/BranchModal.vue';
import TagsModal from './components/TagsModal.vue';
import RemotesModal from './components/RemotesModal.vue';
import ErrorBanner from './components/ErrorBanner.vue';
import LoadingOverlay from './components/LoadingOverlay.vue';
import InputModal from './components/InputModal.vue';

// Helper functions
import { getRepoName } from './utils/path';
import { useOperationMutex } from './composables/useOperationMutex';

// Initialize stores
const repoStore = useRepoStore();
const uiStore = useUIStore();
const settingsStore = useSettingsStore();
const { withLock } = useOperationMutex();

onErrorCaptured((err, _instance, info) => {
  console.error('Error captured in component:', err, info);
  toast.error(err instanceof Error ? err.message : String(err), { title: 'Component Error' });
  return false;
});

const toast = useToast();
const { showContextMenu, hideContextMenu, isVisible, position, menuItems } = useContextMenu();

// ── InputModal state ──
interface InputModalState {
  visible: boolean;
  title: string;
  label: string;
  placeholder: string;
  inputType: 'text' | 'textarea';
  required: boolean;
  secondLabel: string;
  secondPlaceholder: string;
  secondDefault: string;
  confirmHandler: ((value: string, secondValue?: string) => void) | null;
}

const inputModal = reactive<InputModalState>({
  visible: false,
  title: '',
  label: '',
  placeholder: '',
  inputType: 'text',
  required: false,
  secondLabel: '',
  secondPlaceholder: '',
  secondDefault: '',
  confirmHandler: null,
});

const openInputModal = (opts: {
  title: string;
  label?: string;
  placeholder?: string;
  inputType?: 'text' | 'textarea';
  required?: boolean;
  secondLabel?: string;
  secondPlaceholder?: string;
  secondDefault?: string;
  onConfirm: (value: string, secondValue?: string) => void;
}) => {
  inputModal.title = opts.title;
  inputModal.label = opts.label || '';
  inputModal.placeholder = opts.placeholder || '';
  inputModal.inputType = opts.inputType || 'text';
  inputModal.required = opts.required ?? false;
  inputModal.secondLabel = opts.secondLabel || '';
  inputModal.secondPlaceholder = opts.secondPlaceholder || '';
  inputModal.secondDefault = opts.secondDefault || '';
  inputModal.confirmHandler = opts.onConfirm;
  inputModal.visible = true;
};

const handleInputModalConfirm = (value: string, secondValue?: string) => {
  inputModal.visible = false;
  inputModal.confirmHandler?.(value, secondValue);
};

const handleInputModalCancel = () => {
  inputModal.visible = false;
  inputModal.confirmHandler = null;
};

// ── Git operations ──
const toggleAllStaged = async () => {
  if (repoStore.fileStatuses.length === 0) return;
  try {
    const paths = repoStore.fileStatuses.map(f => f.path);
    if (repoStore.allStaged) {
      await gitService.unstageFiles(paths);
    } else {
      const result: StageResult = await gitService.stageFiles(paths);
      if (result.warnings.length > 0) {
        uiStore.setError(result.warnings.join('\n'));
      }
    }
    await repoStore.refreshRepo();
  } catch (err) {
    uiStore.setError(String(err));
  }
};

const handleDiscardAllChanges = async () => {
  const confirmed = await ask("Are you sure you want to discard ALL changes? This cannot be undone.", {
    title: 'Discard All Changes',
    kind: 'warning'
  });
  if (!confirmed) return;
  try {
    uiStore.setLoading(true, "Discarding all changes...", false);
    await gitService.discardAllChanges();
    repoStore.selectedFile = null;
    await repoStore.refreshRepo();
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handleDiscardAllChanges();
  } finally {
    uiStore.setLoading(false);
  }
};

const refreshRepo = async () => {
  await repoStore.refreshRepo();
  if (repoStore.conflicts.length > 0 && uiStore.view !== "conflicts") {
    uiStore.setView("conflicts");
  }
  if (uiStore.view === "history") {
    try {
      await repoStore.refreshCommits();
    } catch (err) {
      console.error('Failed to refresh commits:', err);
      toast.error('Failed to load commit history: ' + String(err), { title: 'Error' });
    }
  }
};

let unlisten: (() => void) | null = null;

onMounted(async () => {
  await settingsStore.fetchSettings();
  try {
    const info = await gitService.getCurrentRepoInfo();
    if (info) {
      repoStore.setRepoInfo(info);
      await refreshRepo();
    }
  } catch (err) {
    console.error("Failed to fetch initial repo info", err);
  }
  unlisten = await listen('git-state-changed', () => {
    refreshRepo().catch(err => console.error('git-state-changed refresh failed:', err));
  });
});

onUnmounted(() => {
  if (unlisten) unlisten();
});

// ── Watchers ──
watch(() => uiStore.view, () => {
  if (repoStore.repoInfo) {
    refreshRepo().catch(err => console.error('View switch refresh failed:', err));
  }
});

watch(() => uiStore.amendCommit, (newVal) => {
  if (newVal && repoStore.commits.length > 0) {
    uiStore.setCommitMessage(repoStore.commits[0].message);
  } else if (!newVal) {
    uiStore.setCommitMessage("");
  }
});

watch(() => repoStore.selectedFile, async (newFile: string | null) => {
  if (newFile && uiStore.view === "changes") {
    await repoStore.setSelectedFile(newFile);
  } else if (!newFile && uiStore.view === "changes") {
    repoStore.diffs = [];
  }
});

watch(() => repoStore.selectedCommit, async (newCommit) => {
  if (newCommit) {
    await repoStore.setSelectedCommit(newCommit);
  } else {
    repoStore.diffs = [];
    repoStore.selectedCommitFile = null;
  }
});

watch(() => uiStore.showRecentRepos, async (isOpen) => {
  if (isOpen && settingsStore.settings?.recent_repositories.length) {
    try {
      const infos = await gitService.getRepositoriesInfo(settingsStore.settings?.recent_repositories || []);
      repoStore.setRecentRepoInfos(infos);
    } catch (err) {
      console.error("Failed to fetch recent repo infos", err);
    }
  }
});

// ── Repo actions ──
const handleOpenRepo = async (path?: string) => {
  try {
    uiStore.setLoading(true, "Opening repository...", true);
    uiStore.clearError();
    let selectedPath = path;
    if (!selectedPath) {
      const selected = await open({ directory: true, multiple: false, title: "Open Repository" });
      if (selected && typeof selected === "string") selectedPath = selected;
    }
    if (selectedPath) {
      const info = await gitService.openRepository(selectedPath);
      repoStore.setRepoInfo(info);
      repoStore.clearSelection();
      await settingsStore.fetchSettings();
      await refreshRepo();
    }
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handleOpenRepo(path);
  } finally {
    uiStore.setLoading(false);
    uiStore.closeModal('recentRepos');
  }
};

const triggerCloneModal = () => uiStore.openModal('clone');

const handleBrowseClonePath = async () => {
  try {
    const selected = await open({ directory: true, multiple: false, title: "Select Clone Destination" });
    if (selected && typeof selected === "string") uiStore.setClonePath(selected);
  } catch (err) {
    uiStore.setError(String(err));
  }
};

const handleCloneRepo = async () => {
  if (!uiStore.cloneUrl || !uiStore.clonePath) return;
  const url = uiStore.cloneUrl;
  const path = uiStore.clonePath;
  uiStore.closeModal('clone');
  try {
    uiStore.setLoading(true, '', true);
    uiStore.clearError();
    await gitService.cloneRepository(url, path);
    const info = await gitService.openRepository(path);
    repoStore.setRepoInfo(info);
    repoStore.clearSelection();
    await settingsStore.fetchSettings();
    await refreshRepo();
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handleCloneRepo();
    setTimeout(() => refreshRepo().catch(err => console.error('Delayed refresh failed:', err)), 500);
  } finally {
    uiStore.setLoading(false);
  }
};

const handlePush = async () => {
  try {
    await withLock('push', async () => {
      uiStore.setLoading(true, "Pushing changes to remote...", true);
      uiStore.clearError();
      await gitService.push();
      toast.success("Pushed successfully!", { title: "Success" });
      await repoStore.refreshRepo();
      uiStore.clearError();
    });
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handlePush();
    setTimeout(() => refreshRepo().catch(err => console.error('Delayed refresh failed:', err)), 500);
  } finally {
    uiStore.setLoading(false);
  }
};

const handlePull = async () => {
  try {
    await withLock('pull', async () => {
      uiStore.setLoading(true, "Pulling from remote...", true);
      uiStore.clearError();
      await gitService.pull();
      toast.success("Pulled successfully!", { title: "Success" });
      await repoStore.refreshRepo();
      uiStore.clearError();
    });
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handlePull();
    setTimeout(() => refreshRepo().catch(err => console.error('Delayed refresh failed:', err)), 500);
  } finally {
    uiStore.setLoading(false);
  }
};

const handleFetch = async () => {
  try {
    await withLock('fetch', async () => {
      uiStore.setLoading(true, "Fetching from remote...", false);
      uiStore.clearError();
      await gitService.fetch();
      toast.success("Fetch completed!", { title: "Success" });
      await repoStore.refreshRepo();
      uiStore.clearError();
    });
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handleFetch();
    setTimeout(() => refreshRepo().catch(err => console.error('Delayed refresh failed:', err)), 500);
  } finally {
    uiStore.setLoading(false);
  }
};

const handleStashSave = () => {
  openInputModal({
    title: 'Save Stash',
    label: 'Stash Message (optional)',
    placeholder: 'Describe your stash...',
    inputType: 'textarea',
    required: false,
    onConfirm: async (message) => {
      try {
        uiStore.setLoading(true, "Saving stash...", false);
        await gitService.stashSave(message || undefined);
        repoStore.selectedFile = null;
        await repoStore.refreshRepo();
        uiStore.clearError();
      } catch (err) {
        uiStore.setError(String(err));
        uiStore.lastFailedOperation = async () => { await gitService.stashSave(message || undefined); };
      } finally {
        uiStore.setLoading(false);
      }
    }
  });
};

const loadTags = async () => {
  try {
    const tagsList = await gitService.listTags();
    uiStore.setTags(tagsList);
  } catch (err) {
    console.error("Failed to load tags", err);
  }
};

const loadRemotes = async () => {
  try {
    const remotesList = await gitService.listRemotes();
    uiStore.setRemotes(remotesList);
  } catch (err) {
    console.error("Failed to load remotes", err);
  }
};

// ── Commit ──
const handleCommit = async () => {
  if (!uiStore.commitMessage.trim()) {
    toast.error("Please enter a commit message", { title: "Commit Error" });
    return;
  }
  if (!uiStore.amendCommit && repoStore.stagedFiles.length === 0) {
    toast.error("Please select files to commit", { title: "Commit Error" });
    return;
  }
  try {
    await withLock('commit', async () => {
      uiStore.setLoading(true, "Creating commit...", true);
      uiStore.clearError();
      if (uiStore.amendCommit) {
        await gitService.amendCommit(uiStore.commitMessage);
        toast.success("Commit amended successfully!", { title: "Success" });
        uiStore.setAmendCommit(false);
      } else {
        await gitService.createCommit(uiStore.commitMessage, repoStore.stagedFiles);
        toast.success("Commit created successfully!", { title: "Success" });
      }
      uiStore.setCommitMessage("");
      repoStore.selectedFile = null;
      await repoStore.refreshRepo();
      uiStore.clearError();
    });
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handleCommit();
  } finally {
    uiStore.setLoading(false);
  }
};

// ── HistoryPanel event handlers ──
const handleRequestCreateBranch = (commit: CommitInfo) => {
  openInputModal({
    title: 'Create Branch',
    label: `Branch from ${commit.sha.substring(0, 7)}`,
    placeholder: 'feature/new-branch',
    required: true,
    onConfirm: async (name) => {
      try {
        uiStore.setLoading(true, '', false);
        await gitService.createBranch(name, commit.sha);
        await repoStore.refreshRepo();
        toast.success(`Created branch ${name}`, { title: 'Success' });
      } catch (e) {
        uiStore.setError(String(e));
      } finally {
        uiStore.setLoading(false);
      }
    }
  });
};

const handleRequestCreateTag = (commit: CommitInfo) => {
  openInputModal({
    title: 'Create Tag',
    label: `Tag for ${commit.sha.substring(0, 7)}`,
    placeholder: 'v1.0.0',
    required: true,
    secondLabel: 'Tag Message (optional)',
    secondPlaceholder: 'Describe this tag...',
    onConfirm: async (tagName, tagMessage) => {
      try {
        uiStore.setLoading(true, '', false);
        await gitService.createTag(tagName, tagMessage || '', commit.sha);
        await repoStore.refreshRepo();
        toast.success(`Created tag ${tagName}`, { title: 'Success' });
      } catch (e) {
        uiStore.setError(String(e));
      } finally {
        uiStore.setLoading(false);
      }
    }
  });
};

// ── Commit detail actions ──
const handleCherryPick = async (sha: string) => {
  const confirmed = await ask(`Cherry-pick commit ${sha.substring(0, 7)}?`, { title: 'Cherry-pick', kind: 'info' });
  if (!confirmed) return;
  try {
    await withLock('cherry-pick', async () => {
      uiStore.setLoading(true, "Cherry-picking commit...", false);
      await gitService.cherryPick(sha);
      await repoStore.refreshRepo();
      toast.success("Cherry-pick successful", { title: "Success" });
      uiStore.clearError();
    });
  } catch (e) {
    uiStore.setError(String(e));
    uiStore.lastFailedOperation = async () => await handleCherryPick(sha);
  } finally {
    uiStore.setLoading(false);
  }
};

const handleRevertCommit = async (sha: string) => {
  const confirmed = await ask(`Revert commit ${sha.substring(0, 7)}?`, { title: 'Revert Commit', kind: 'warning' });
  if (!confirmed) return;
  try {
    await withLock('revert', async () => {
      uiStore.setLoading(true, "Reverting commit...", false);
      await gitService.revertCommit(sha);
      await repoStore.refreshRepo();
      toast.success("Revert successful", { title: "Success" });
      uiStore.clearError();
    });
  } catch (e) {
    uiStore.setError(String(e));
    uiStore.lastFailedOperation = async () => await handleRevertCommit(sha);
  } finally {
    uiStore.setLoading(false);
  }
};

// ── StashPanel event handler ──
const handleRequestStashBranch = (stash: { sha: string; message: string }) => {
  openInputModal({
    title: 'Create Branch from Stash',
    label: `Branch from stash ${stash.sha.substring(0, 7)}`,
    placeholder: 'stash-branch',
    required: true,
    onConfirm: async (branchName) => {
      try {
        uiStore.setLoading(true, "Creating branch from stash...", false);
        await gitService.branchFromStash(stash.sha, branchName);
        await repoStore.refreshRepo();
        toast.success(`Branch ${branchName} created from stash`, { title: 'Success' });
      } catch (err) {
        uiStore.setError(String(err));
        uiStore.lastFailedOperation = async () => await gitService.branchFromStash(stash.sha, branchName);
      } finally {
        uiStore.setLoading(false);
      }
    }
  });
};

// ── Commit file context menu ──
const onCommitFileContextMenu = (event: MouseEvent, filePath: string) => {
  showContextMenu(event, [
    {
      label: 'Copy Path',
      action: async () => {
        try {
          await navigator.clipboard.writeText(filePath);
          toast.success('Path copied', { title: 'Copied' });
        } catch {
          toast.error('Failed to copy path', { title: 'Clipboard Error' });
        }
      }
    },
    {
      label: 'View File History',
      action: () => uiStore.setSearchCommitQuery(filePath)
    },
    { divider: true },
    {
      label: 'Reveal in Finder/Explorer',
      action: async () => {
        if (repoStore.repoInfo) {
          try {
            await gitService.revealInFinder(`${repoStore.repoInfo.path}/${filePath}`);
          } catch (e) {
            uiStore.setError(String(e));
          }
        }
      }
    },
    {
      label: 'Open in Editor',
      action: async () => {
        if (repoStore.repoInfo) {
          try {
            const { openPath } = await import('@tauri-apps/plugin-opener');
            await openPath(`${repoStore.repoInfo.path}/${filePath}`);
          } catch (e) {
            uiStore.setError(String(e));
          }
        }
      }
    }
  ]);
};

// ── Keyboard shortcuts ──
useKeyboardShortcuts([
  { key: 's', ctrl: true, action: () => uiStore.view === 'changes' && handleCommit(), description: 'Commit staged changes' },
  { key: 'p', ctrl: true, action: () => repoStore.repoInfo && handlePush(), description: 'Push changes' },
  { key: 'P', ctrl: true, action: () => repoStore.repoInfo && handlePull(), description: 'Pull changes' },
  { key: 'f', ctrl: true, action: () => repoStore.repoInfo && handleFetch(), description: 'Fetch from remote' },
  { key: 'b', ctrl: true, action: () => repoStore.repoInfo && uiStore.openModal('branch'), description: 'Open branch switcher' },
  { key: 'k', ctrl: true, action: () => repoStore.repoInfo && handleStashSave(), description: 'Stash changes' },
  { key: 'Escape', action: () => { uiStore.closeAllModals(); inputModal.visible = false; }, description: 'Close modal' },
]);
</script>

<template>
  <Toast />
  <ContextMenu :visible="isVisible" :position="position" :items="menuItems" @close="hideContextMenu" />
  <InputModal
    :visible="inputModal.visible"
    :title="inputModal.title"
    :label="inputModal.label"
    :placeholder="inputModal.placeholder"
    :input-type="inputModal.inputType"
    :required="inputModal.required"
    :second-label="inputModal.secondLabel"
    :second-placeholder="inputModal.secondPlaceholder"
    :second-default-value="inputModal.secondDefault"
    @confirm="handleInputModalConfirm"
    @cancel="handleInputModalCancel"
  />
  <div class="app flex flex-col h-screen bg-background text-foreground overflow-hidden font-sans">
    <!-- Header -->
    <HeaderBar 
      @openRepo="handleOpenRepo"
      @triggerCloneModal="triggerCloneModal"
      @handleFetch="handleFetch"
      @loadTags="loadTags"
      @loadRemotes="loadRemotes"
    />

    <!-- Error Banner -->
    <ErrorBanner />

    <!-- Modals -->
    <div v-if="uiStore.showCloneModal || uiStore.showSettingsModal || uiStore.showBranchModal || uiStore.showTagsModal || uiStore.showRemotesModal" class="fixed inset-0 flex items-center justify-center z-[100] p-4" style="background: rgba(0,0,0,0.65); backdrop-filter: blur(8px);">
      <CloneModal v-if="uiStore.showCloneModal" @browse="handleBrowseClonePath" @clone="handleCloneRepo" />
      <SettingsModal v-if="uiStore.showSettingsModal" />
      <BranchModal v-if="uiStore.showBranchModal" @checkout="(name) => gitService.checkoutBranch(name).then(() => { uiStore.closeModal('branch'); repoStore.refreshRepo(); }).catch(err => { uiStore.setError(String(err)); })" @createBranch="() => gitService.createBranch(uiStore.newBranchName).then(() => { uiStore.setNewBranchName(''); uiStore.closeModal('branch'); repoStore.refreshRepo(); }).catch(err => { uiStore.setError(String(err)); })" />
      <TagsModal v-if="uiStore.showTagsModal" />
      <RemotesModal v-if="uiStore.showRemotesModal" />
    </div>

    <!-- Main Content Area -->
    <div v-if="repoStore.repoInfo" class="flex flex-1 overflow-hidden">
      <!-- Left Sidebar -->
      <aside class="w-80 min-w-[20rem] max-w-[20rem] flex-shrink-0 border-r border-border flex flex-col bg-card shadow-sm">
        <div class="flex border-b text-[12px] font-semibold" style="border-color: var(--border);">
          <button @click="uiStore.setView('changes')"
            class="flex-1 py-2.5 transition-safe relative border-r"
            style="border-color: var(--border);"
            :style="uiStore.view === 'changes' ? 'color: var(--accent);' : 'color: var(--muted-foreground);'">
            Changes
            <span v-if="repoStore.fileStatuses.length > 0" class="ml-1 badge" style="background:var(--spotlight);color:var(--accent);">{{ repoStore.fileStatuses.length }}</span>
            <span v-if="uiStore.view === 'changes'" class="absolute bottom-0 left-0 right-0 h-0.5 gradient-bg"></span>
          </button>
          <button @click="uiStore.setView('history')"
            class="flex-1 py-2.5 transition-safe relative"
            :class="{ 'border-r': repoStore.stashes.length > 0 || repoStore.conflicts.length > 0 }"
            style="border-color: var(--border);"
            :style="uiStore.view === 'history' ? 'color: var(--accent);' : 'color: var(--muted-foreground);'">History
            <span v-if="uiStore.view === 'history'" class="absolute bottom-0 left-0 right-0 h-0.5 gradient-bg"></span>
          </button>
          <button v-if="repoStore.stashes.length > 0" @click="uiStore.setView('stashes')"
            class="flex-1 py-2.5 transition-safe relative border-r"
            style="border-color: var(--border);"
            :style="uiStore.view === 'stashes' ? 'color: var(--accent);' : 'color: var(--muted-foreground);'">Stash
            <span v-if="uiStore.view === 'stashes'" class="absolute bottom-0 left-0 right-0 h-0.5 gradient-bg"></span>
          </button>
          <button v-if="repoStore.conflicts.length > 0" @click="uiStore.setView('conflicts')"
            class="flex-1 py-2.5 transition-safe relative"
            style="color: var(--error);">Conflicts
            <span v-if="uiStore.view === 'conflicts'" class="absolute bottom-0 left-0 right-0 h-0.5" style="background: var(--error);"></span>
          </button>
        </div>

        <div class="flex-1 overflow-auto p-3">
          <ChangesPanel v-if="uiStore.view === 'changes'" @toggleAllStaged="toggleAllStaged" @handleDiscardAllChanges="handleDiscardAllChanges" />
          <HistoryPanel v-else-if="uiStore.view === 'history'" @requestCreateBranch="handleRequestCreateBranch" @requestCreateTag="handleRequestCreateTag" />
          <StashPanel v-else-if="uiStore.view === 'stashes'" @requestStashBranch="handleRequestStashBranch" />
          <ConflictsPanel v-else-if="uiStore.view === 'conflicts'" />
        </div>

        <CommitPanel v-if="uiStore.view === 'changes'" @commit="handleCommit" />

        <div class="p-3 border-t border-border flex gap-2 overflow-x-auto bg-card text-sm">
          <button @click="handlePull" class="flex-1 bg-card border border-border py-2 px-3 rounded-lg hover:bg-muted transition-safe font-medium flex items-center justify-center gap-2">
            Pull
            <span v-if="repoStore.repoInfo?.behind" class="flex items-center justify-center bg-error/10 text-error text-[10px] w-4 h-4 rounded-full font-bold">{{ repoStore.repoInfo.behind }}</span>
          </button>
          <button @click="handlePush" class="flex-1 bg-card border border-border py-2 px-3 rounded-lg hover:bg-muted transition-safe font-medium flex items-center justify-center gap-2">
            Push
            <span v-if="repoStore.repoInfo?.ahead" class="flex items-center justify-center bg-success/10 text-success text-[10px] w-4 h-4 rounded-full font-bold">{{ repoStore.repoInfo.ahead }}</span>
          </button>
          <button @click="handleStashSave" class="flex-1 bg-card border border-border py-2 px-3 rounded-lg hover:bg-muted transition-safe font-medium">Stash</button>
          <button v-if="uiStore.view === 'history' && repoStore.selectedCommit" @click="repoStore.selectedCommit = null" class="flex-1 bg-card border border-border py-2 px-3 rounded-lg hover:bg-error/10 hover:text-error transition-safe font-medium">Clear</button>
        </div>
      </aside>

      <!-- Diff/Main View -->
      <main class="flex-1 bg-background flex flex-col overflow-hidden">
        <div v-if="uiStore.view === 'changes' && repoStore.selectedFile" class="flex-1 flex flex-col overflow-hidden">
          <div class="h-12 border-b border-border flex items-center px-6 bg-card text-sm font-mono text-muted-foreground">
            {{ repoStore.selectedFile }}
          </div>
          <div class="flex-1 overflow-auto">
            <DiffPanel :diffs="repoStore.diffs" />
          </div>
        </div>
        <div v-else-if="uiStore.view === 'history' && repoStore.selectedCommit" class="flex-1 flex flex-col overflow-hidden">
          <div class="h-14 border-b border-border flex items-center px-6 bg-card text-sm font-mono justify-between flex-shrink-0">
            <div class="flex items-center gap-3 overflow-hidden">
              <span class="text-accent font-semibold flex-shrink-0">{{ repoStore.selectedCommit.sha.substring(0, 7) }}</span>
              <span class="text-muted-foreground truncate" :title="repoStore.selectedCommit.message">{{ repoStore.selectedCommit.message }}</span>
            </div>
            <div class="flex items-center gap-3 flex-shrink-0 ml-4">
               <button @click="handleCherryPick(repoStore.selectedCommit.sha)" class="px-3 py-1.5 border border-border rounded text-xs hover:bg-muted transition-safe font-medium" title="Apply this commit to current branch">Cherry-pick</button>
               <button @click="handleRevertCommit(repoStore.selectedCommit.sha)" class="px-3 py-1.5 border border-border rounded text-xs hover:bg-muted hover:text-error transition-safe font-medium" title="Create a new commit that reverts this one">Revert</button>
            </div>
          </div>
          
          <div class="flex-1 flex overflow-hidden">
            <!-- Left: File List -->
            <div class="w-64 border-r border-border bg-card overflow-y-auto flex-shrink-0">
              <div v-for="diff in repoStore.diffs" :key="diff.path"
                   @click="repoStore.selectedCommitFile = diff.path"
                   @contextmenu.prevent="onCommitFileContextMenu($event, diff.path)"
                   class="px-4 py-2 text-sm cursor-pointer border-l-2 hover:bg-muted transition-safe flex items-center justify-between group"
                   :class="{ 'border-accent bg-accent/5': repoStore.selectedCommitFile === diff.path, 'border-transparent': repoStore.selectedCommitFile !== diff.path }">
                <span class="truncate" :title="diff.path">{{ diff.path.split('/').pop() }}</span>
                <span class="text-xs w-4 text-center font-bold" 
                      :class="{ 'text-success': diff.additions > 0 && diff.deletions === 0, 'text-error': diff.deletions > 0 && diff.additions === 0, 'text-accent': diff.additions > 0 && diff.deletions > 0 }">
                  {{ diff.additions > 0 && diff.deletions === 0 ? 'A' : (diff.deletions > 0 && diff.additions === 0 ? 'D' : 'M') }}
                </span>
              </div>
            </div>

            <!-- Right: Diff -->
            <div class="flex-1 overflow-auto bg-background">
              <DiffPanel :diffs="repoStore.diffs.filter(d => d.path === repoStore.selectedCommitFile)" />
            </div>
          </div>
        </div>
        <div v-else class="flex-1 flex items-center justify-center text-muted-foreground text-sm">
          {{ uiStore.view === 'history' ? 'Select a commit to view diff' : 'Select a file to view changes' }}
        </div>
      </main>
    </div>

    <!-- Welcome View -->
    <div v-else class="flex-1 flex flex-col items-center justify-center bg-background grid-pattern relative overflow-hidden">
      <div class="relative max-w-lg w-full text-center px-8" style="animation: fade-in-up 0.4s ease;">
        <!-- Logo mark -->
        <div class="flex justify-center mb-7">
          <div class="relative">
            <div class="w-16 h-16 rounded-2xl flex items-center justify-center" style="background: var(--foreground);">
              <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" style="stroke: var(--accent-foreground);">
                <path d="M12 2L2 7l10 5 10-5-10-5z"/>
                <path d="M2 17l10 5 10-5"/>
                <path d="M2 12l10 5 10-5"/>
              </svg>
            </div>
            <!-- Amber dot -->
            <div class="absolute -top-1 -right-1 w-3.5 h-3.5 rounded-full border-2" style="background: var(--mark); border-color: var(--background);"></div>
          </div>
        </div>

        <h1 class="text-4xl font-semibold tracking-tight mb-2" style="color: var(--foreground); letter-spacing: -0.03em;">Ark</h1>
        <p class="text-[14px] mb-10" style="color: var(--muted-foreground);">A precision Git client for professional workflows</p>

        <div class="grid grid-cols-2 gap-3 mb-8">
          <button @click="handleOpenRepo()" class="group p-5 rounded-xl border text-left transition-safe hover:bg-muted"
            style="background: var(--card); border-color: var(--border);">
            <div class="w-8 h-8 rounded-lg flex items-center justify-center mb-3" style="background: var(--muted-foreground); opacity:0.15;">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="stroke: var(--foreground); opacity:1;">
                <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
              </svg>
            </div>
            <div class="font-medium text-[14px] mb-0.5" style="color: var(--foreground);">Open</div>
            <div class="text-[12px]" style="color: var(--muted-foreground);">Load from filesystem</div>
          </button>
          <button @click="triggerCloneModal" class="group p-5 rounded-xl border text-left transition-safe hover:bg-muted"
            style="background: var(--card); border-color: var(--border);">
            <div class="w-8 h-8 rounded-lg flex items-center justify-center mb-3" style="background: var(--muted);">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="stroke: var(--mark);">
                <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
                <polyline points="7 10 12 15 17 10"/>
                <line x1="12" y1="15" x2="12" y2="3"/>
              </svg>
            </div>
            <div class="font-medium text-[14px] mb-0.5" style="color: var(--foreground);">Clone</div>
            <div class="text-[12px]" style="color: var(--muted-foreground);">From remote URL</div>
          </button>
        </div>

        <div v-if="settingsStore.settings?.recent_repositories.length" class="text-left">
          <div class="flex items-center gap-3 mb-3">
            <div class="h-px flex-1" style="background: var(--border);"></div>
            <span class="text-[11px] font-semibold uppercase tracking-widest" style="color: var(--muted-foreground);">Recent</span>
            <div class="h-px flex-1" style="background: var(--border);"></div>
          </div>
          <div class="space-y-1.5">
            <div v-for="path in settingsStore.settings.recent_repositories.slice(0, 5)" :key="path"
                 @click="handleOpenRepo(path)"
                 class="group flex items-center gap-3 px-4 py-3 rounded-xl border cursor-pointer transition-safe hover:border-[var(--accent)]"
                 style="background: var(--card); border-color: var(--border);">
              <div class="w-7 h-7 rounded-lg flex items-center justify-center flex-shrink-0 text-[11px] font-bold" style="background:var(--muted); color:var(--accent);">{{ getRepoName(path)[0]?.toUpperCase() }}</div>
              <div class="flex-1 min-w-0">
                <div class="font-semibold truncate text-[13px]">{{ getRepoName(path) }}</div>
                <div class="text-[11px] font-mono truncate" style="color:var(--muted-foreground);">{{ path }}</div>
              </div>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="flex-shrink-0 transition-safe group-hover:text-[var(--accent)]" style="color:var(--muted-foreground);">
                <polyline points="9 18 15 12 9 6"/>
              </svg>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Loading Overlay -->
    <LoadingOverlay />
  </div>
</template>

<style>
/* Minimalist modern styles defined in index.css */
</style>
