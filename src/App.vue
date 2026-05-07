<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, computed, onErrorCaptured } from 'vue';
import { gitService, type RepositoryInfo, type FileStatus, type BranchInfo, type CommitInfo, type StashInfo, type ConflictInfo, type StageResult } from './services/git';
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

// Import stores
import { useRepoStore } from './stores/repo';
import { useUIStore } from './stores/ui';
import { useSettingsStore } from './stores/settings';

// Initialize stores
const repoStore = useRepoStore();
const uiStore = useUIStore();
const settingsStore = useSettingsStore();

// Memory optimization: reuse refs and limit array growth
const MAX_TIMEOUTS = 10;
const abortController = ref<AbortController | null>(null);

onErrorCaptured((err, instance, info) => {
  console.error('Error captured in component:', err, info);
  toast.error(err instanceof Error ? err.message : String(err), { title: 'Component Error' });
  return false;
});

// Abort all pending operations on unmount
const abortPendingOperations = () => {
  if (abortController.value) {
    abortController.value.abort();
  }
};

const retryLastOperation = async () => {
  if (uiStore.lastFailedOperation) {
    const op = uiStore.lastFailedOperation;
    await op();
  }
};

const trackedSetTimeout = (callback: () => void, delay: number) => {
  const id = setTimeout(() => {
    timeoutIds.value = timeoutIds.value.filter(t => t !== id);
    callback();
  }, delay);
  timeoutIds.value.push(id);
  return id;
};

const clearTimeouts = () => {
  timeoutIds.value.forEach(id => clearTimeout(id));
  timeoutIds.value = [];
};

const timeoutIds = ref<ReturnType<typeof setTimeout>[]>([]);

const error = ref<string | null>(null);
const toast = useToast();
const { showContextMenu, hideContextMenu, isVisible, position, menuItems } = useContextMenu();

const dropdownRef = ref<HTMLElement | null>(null);

// Computed properties that use store state
const currentProjectName = computed(() => {
  if (!repoStore.repoInfo) return "";
  
  const path = repoStore.repoInfo.path;
  const name = getRepoName(path);
  
  // 額外驗證：如果得到的名字和分支名相同，可能路徑有問題
  if (name === repoStore.repoInfo.current_branch) {
    console.warn('[WARNING] Project name equals branch name, path might be incorrect:', path);
    if (path && (path.includes('/') || path.includes('\\'))) {
      return getRepoName(path);
    }
    return path || "";
  }
  
  return name;
});

watch(currentProjectName, (name) => {
  document.title = name ? `Ark - ${name}` : "Ark";
}, { immediate: true });

const filteredCommits = computed(() => {
  if (!uiStore.searchCommitQuery.trim()) return repoStore.commits;
  const q = uiStore.searchCommitQuery.toLowerCase();
  return repoStore.commits.filter(c => 
    c.message.toLowerCase().includes(q) || 
    c.sha.toLowerCase().includes(q) || 
    c.author.toLowerCase().includes(q)
  );
});

const getRepoName = (path: string) => {
  if (!path || path.trim() === "") return "";
  
  // 移除尾部的斜線
  const cleanPath = path.replace(/[/\\]+$/, '');
  
  // 如果路徑以 .git 結尾，取父目錄名
  if (cleanPath.endsWith('.git')) {
    const withoutGit = cleanPath.slice(0, -4).replace(/[/\\]+$/, '');
    const parts = withoutGit.split(/[/\\]/);
    return parts[parts.length - 1] || "";
  }
  
  // 取最後一個路徑段
  const parts = cleanPath.split(/[/\\]/);
  const lastPart = parts[parts.length - 1];
  
  // 如果最後一部分看起來不像是目錄名（太短或只是一個點），返回倒數第二個
  if (!lastPart || lastPart === '.' || lastPart === '..') {
    return parts[parts.length - 2] || path;
  }
  
  return lastPart || path;
};

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

const onCommitContextMenu = (event: MouseEvent, commit: CommitInfo) => {
  showContextMenu(event, [
    {
      label: 'Copy SHA',
      action: async () => {
        await navigator.clipboard.writeText(commit.sha.substring(0, 7));
        toast.success('SHA copied', { title: 'Copied' });
      }
    },
    {
      label: 'Copy Full SHA',
      action: async () => {
        await navigator.clipboard.writeText(commit.sha);
        toast.success('Full SHA copied', { title: 'Copied' });
      }
    },
    {
      label: 'Copy Commit Message',
      action: async () => {
        await navigator.clipboard.writeText(commit.message);
        toast.success('Message copied', { title: 'Copied' });
      }
    },
    { divider: true },
    {
      label: 'Create Branch from Commit',
      action: async () => {
        const name = prompt(`Enter new branch name from ${commit.sha.substring(0, 7)}:`);
        if (name && name.trim()) {
          try {
            uiStore.setLoading(true, '', false);
            await gitService.createBranch(name.trim(), commit.sha);
            await repoStore.refreshRepo();
            toast.success(`Created branch ${name}`, { title: 'Success' });
          } catch (e) {
            uiStore.setError(String(e));
          } finally {
            uiStore.setLoading(false);
          }
        }
      }
    },
    {
      label: 'Create Tag',
      action: async () => {
        const tagName = prompt(`Enter tag name for ${commit.sha.substring(0, 7)}:`);
        if (tagName && tagName.trim()) {
          const tagMessage = prompt(`Enter tag message (optional):`) || '';
          try {
            uiStore.setLoading(true, '', false);
            await gitService.createTag(tagName.trim(), tagMessage, commit.sha);
            await repoStore.refreshRepo();
            toast.success(`Created tag ${tagName}`, { title: 'Success' });
          } catch (e) {
            uiStore.setError(String(e));
          } finally {
            uiStore.setLoading(false);
          }
        }
      }
    },
    {
      label: 'Cherry-pick Commit',
      action: () => handleCherryPick(commit.sha)
    },
    {
      label: 'Revert Commit',
      danger: true,
      action: () => handleRevertCommit(commit.sha)
    },
    {
      label: 'Reset Branch to this Commit',
      danger: true,
      action: async () => {
        const confirmed = await ask(`Reset current branch to ${commit.sha.substring(0, 7)}? This will discard all commits after this point.`, { 
          title: 'Reset Branch', 
          kind: 'warning' 
        });
        if (!confirmed) return;
        try {
          uiStore.setLoading(true, "Resetting branch...", false);
          await gitService.resetBranch(commit.sha);
          await repoStore.refreshRepo();
          toast.success("Branch reset successfully", { title: 'Success' });
          uiStore.clearError();
        } catch (e) {
          uiStore.setError(String(e));
          uiStore.lastFailedOperation = async () => await gitService.resetBranch(commit.sha);
        } finally {
          uiStore.setLoading(false);
        }
      }
    },
    {
      label: 'Merge into Current Branch',
      action: async () => {
        const confirmed = await ask(`Merge commit ${commit.sha.substring(0, 7)} into current branch?`, { 
          title: 'Merge Commit', 
          kind: 'info' 
        });
        if (!confirmed) return;
        try {
          uiStore.setLoading(true, "Merging commit...", false);
          await gitService.mergeCommit(commit.sha);
          await repoStore.refreshRepo();
          toast.success("Merge successful", { title: 'Success' });
          uiStore.clearError();
        } catch (e) {
          uiStore.setError(String(e));
          uiStore.lastFailedOperation = async () => await gitService.mergeCommit(commit.sha);
        } finally {
          uiStore.setLoading(false);
        }
      }
    },
    { divider: true },
    {
      label: 'View on GitHub',
      action: async () => {
        try {
          let url = await gitService.getRemoteUrl("origin");
          if (url) {
            if (url.startsWith('git@github.com:')) {
               url = url.replace('git@github.com:', 'https://github.com/').replace(/\.git$/, '');
            } else if (url.startsWith('https://')) {
               url = url.replace(/\.git$/, '');
            }
            await openUrl(`${url}/commit/${commit.sha}`);
          } else {
            toast.error("No origin remote found", { title: "Error" });
          }
        } catch (e) {
          uiStore.setError(String(e));
        }
      }
    },
    { divider: true },
    {
      label: 'Reveal in Finder/Explorer',
      action: async () => {
        if (repoStore.repoInfo) {
          try {
            await gitService.revealInFinder(repoStore.repoInfo.path);
          } catch (e) {
            uiStore.setError(String(e));
          }
        }
      }
    }
  ]);
};

const onFileContextMenu = (event: MouseEvent, file: FileStatus) => {
  showContextMenu(event, [
    {
      label: file.staged ? 'Unstage File' : 'Stage File',
      action: () => toggleStaged(file)
    },
    { divider: true },
    {
      label: 'Discard Changes',
      danger: true,
      action: () => handleDiscardChanges(file.path)
    },
    { divider: true },
    {
      label: 'Copy Path',
      action: async () => {
        try {
          await navigator.clipboard.writeText(file.path);
          toast.success('Path copied', { title: 'Copied' });
        } catch (err) {
          console.error('Clipboard error:', err);
        }
      }
    },
    {
      label: 'Ignore File',
      action: async () => {
        try {
          await gitService.addToGitignore(file.path);
          await repoStore.refreshRepo();
          toast.success(`Added ${file.path} to .gitignore`, { title: 'Success' });
        } catch (e) {
          uiStore.setError(String(e));
        }
      }
    },
    {
      label: 'View File History',
      action: async () => {
        uiStore.setView("history");
        uiStore.setSearchCommitQuery(file.path);
      }
    },
    {
      label: 'Copy File Contents',
      action: async () => {
        try {
          if (repoStore.repoInfo) {
            const content = await gitService.readFile(`${repoStore.repoInfo.path}/${file.path}`);
            await navigator.clipboard.writeText(content);
            toast.success('File contents copied', { title: 'Copied' });
          }
        } catch (e) {
          uiStore.setError(String(e));
        }
      }
    },
    { divider: true },
    {
      label: 'Reveal in Finder/Explorer',
      action: async () => {
        if (repoStore.repoInfo) {
          try {
            await gitService.revealInFinder(`${repoStore.repoInfo.path}/${file.path}`);
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
            await openPath(`${repoStore.repoInfo.path}/${file.path}`);
          } catch (e) {
            uiStore.setError(String(e));
          }
        }
      }
    }
  ]);
};

const onFileHeaderContextMenu = (event: MouseEvent) => {
  if (repoStore.fileStatuses.length === 0) return;
  showContextMenu(event, [
    {
      label: repoStore.allStaged ? 'Unstage All' : 'Stage All',
      action: () => toggleAllStaged()
    },
    { divider: true },
    {
      label: 'Discard All Changes',
      danger: true,
      action: () => handleDiscardAllChanges()
    }
  ]);
};

const onCommitFileContextMenu = (event: MouseEvent, filePath: string) => {
  showContextMenu(event, [
    {
      label: 'Copy Path',
      action: async () => {
        await navigator.clipboard.writeText(filePath);
        toast.success('Path copied', { title: 'Copied' });
      }
    },
    {
      label: 'View File History',
      action: async () => {
        uiStore.setSearchCommitQuery(filePath);
      }
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
            await openPath(`${repoStore.repoInfo.path}/${filePath}`);
          } catch (e) {
            uiStore.setError(String(e));
          }
        }
      }
    }
  ]);
};

const onStashContextMenu = (event: MouseEvent, stash: StashInfo) => {
  showContextMenu(event, [
    {
      label: 'Apply Stash',
      action: () => handleApplyStash(stash.index)
    },
    {
      label: 'Pop Stash',
      action: () => handleStashPop(stash.index)
    },
    {
      label: 'Drop Stash',
      danger: true,
      action: () => handleDropStash(stash.index)
    },
    { divider: true },
    {
      label: 'Create Branch from Stash',
      action: () => handleBranchFromStash(stash.index)
    },
    {
      label: 'Copy SHA',
      action: async () => {
        await navigator.clipboard.writeText(stash.sha);
        toast.success('SHA copied', { title: 'Copied' });
      }
    }
  ]);
};

const onConflictContextMenu = (event: MouseEvent, conflict: ConflictInfo) => {
  showContextMenu(event, [
    {
      label: 'Use Ours',
      action: () => handleResolve(conflict.path, true)
    },
    {
      label: 'Use Theirs',
      action: () => handleResolve(conflict.path, false)
    },
    { divider: true },
    {
      label: 'Copy Path',
      action: async () => {
        await navigator.clipboard.writeText(conflict.path);
        toast.success('Path copied', { title: 'Copied' });
      }
    },
    {
      label: 'Reveal in Finder/Explorer',
      action: async () => {
        if (repoStore.repoInfo) {
          try {
            await gitService.revealInFinder(`${repoStore.repoInfo.path}/${conflict.path}`);
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
            await openPath(`${repoStore.repoInfo.path}/${conflict.path}`);
          } catch (e) {
            uiStore.setError(String(e));
          }
        }
      }
    }
  ]);
};

const refreshRepo = async () => {
  await repoStore.refreshRepo();
  
  // Check for conflicts and switch view if needed
  if (repoStore.conflicts.length > 0 && uiStore.view !== "conflicts") {
    uiStore.setView("conflicts");
  }
  
  // Load commits if in history view
  if (uiStore.view === "history" && repoStore.commits.length === 0) {
    await repoStore.refreshCommits();
  }
};

let unlisten: (() => void) | null = null;

onMounted(async () => {
  window.addEventListener('click', handleClickOutside);
  abortController.value = new AbortController();
  
  await settingsStore.fetchSettings();
  
  try {
    const info = await gitService.getCurrentRepoInfo();
    if (info) {
      repoStore.setRepoInfo(info);
    }
  } catch (err) {
    console.error("Failed to fetch initial repo info", err);
  }

  unlisten = await listen('git-state-changed', () => {
    refreshRepo();
  });
});

onUnmounted(() => {
  if (unlisten) {
    unlisten();
  }
  window.removeEventListener('click', handleClickOutside);
  clearTimeouts();
  abortPendingOperations();
});

// Watchers
watch([repoStore.repoInfo, uiStore.view], () => {
  if (repoStore.repoInfo) {
    refreshRepo();
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

watch(uiStore.showRecentRepos, async (isOpen) => {
  if (isOpen && settingsStore.settings?.recent_repositories.length) {
    try {
      const infos = await gitService.getRepositoriesInfo(settingsStore.settings?.recent_repositories || []);
      repoStore.setRecentRepoInfos(infos);
    } catch (err) {
      console.error("Failed to fetch recent repo infos", err);
    }
  }
});

watch(() => uiStore.cloneUrl, async (newUrl) => {
  if (newUrl) {
    const match = newUrl.match(/\/([^\/]+?)(\.git)?$/);
    if (match && match[1]) {
      const repoName = match[1];
      
      let basePath = "";
      if (repoStore.repoInfo) {
        basePath = repoStore.repoInfo.path.substring(0, repoStore.repoInfo.path.lastIndexOf('/'));
      } else if (settingsStore.settings && settingsStore.settings.recent_repositories.length > 0) {
        const lastRepo = settingsStore.settings.recent_repositories[0];
        basePath = lastRepo.substring(0, lastRepo.lastIndexOf('/'));
      }

      if (!basePath || !basePath.includes('github')) {
        try {
          const home = await homeDir();
          basePath = `${home}/Documents/github`;
        } catch {
          basePath = "";
        }
      }
      
      uiStore.setClonePath(`${basePath}/${repoName}`);
    }
  }
});

const handleOpenRepo = async (path?: string) => {
  try {
    uiStore.setLoading(true, "Opening repository...", true);
    uiStore.clearError();

    let selectedPath = path;
    if (!selectedPath) {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Open Repository",
      });
      if (selected && typeof selected === "string") {
        selectedPath = selected;
      }
    }

    if (selectedPath) {
      const info = await gitService.openRepository(selectedPath);
      repoStore.setRepoInfo(info);
      repoStore.clearSelection();
      await settingsStore.fetchSettings();
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

const triggerCloneModal = () => {
  uiStore.openModal('clone');
};

const handleBrowseClonePath = async () => {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Select Clone Destination",
    });
    if (selected && typeof selected === "string") {
      uiStore.setClonePath(selected);
    }
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
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handleCloneRepo();
    trackedSetTimeout(() => refreshRepo(), 500);
  } finally {
    uiStore.setLoading(false);
  }
};

const toggleStaged = async (file: FileStatus) => {
  try {
    if (file.staged) {
      await gitService.unstageFiles([file.path]);
    } else {
      const result: StageResult = await gitService.stageFiles([file.path]);
      if (result.warnings.length > 0) {
        uiStore.setError(result.warnings.join('\n'));
      }
    }
    await repoStore.refreshRepo();
  } catch (err) {
    uiStore.setError(String(err));
  }
};

const handleDiscardChanges = async (path: string) => {
  const confirmed = await ask(`Are you sure you want to discard changes in ${path}? This cannot be undone.`, { 
    title: 'Discard Changes',
    kind: 'warning'
  });
  if (!confirmed) return;
  try {
    uiStore.setLoading(true, "Discarding changes...", false);
    await gitService.discardChanges(path);
    if (repoStore.selectedFile === path) repoStore.selectedFile = null;
    await repoStore.refreshRepo();
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handleDiscardChanges(path);
  } finally {
    uiStore.setLoading(false);
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
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handleCommit();
  } finally {
    uiStore.setLoading(false);
  }
};

const handleCherryPick = async (sha: string) => {
  const confirmed = await ask(`Cherry-pick commit ${sha.substring(0, 7)}?`, { title: 'Cherry-pick', kind: 'info' });
  if (!confirmed) return;
  try {
    uiStore.setLoading(true, "Cherry-picking commit...", false);
    await gitService.cherryPick(sha);
    await repoStore.refreshRepo();
    toast.success("Cherry-pick successful", { title: "Success" });
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handleCherryPick(sha);
  } finally {
    uiStore.setLoading(false);
  }
};

const handleRevertCommit = async (sha: string) => {
  const confirmed = await ask(`Revert commit ${sha.substring(0, 7)}?`, { title: 'Revert Commit', kind: 'warning' });
  if (!confirmed) return;
  try {
    uiStore.setLoading(true, "Reverting commit...", false);
    await gitService.revertCommit(sha);
    await repoStore.refreshRepo();
    toast.success("Revert successful", { title: "Success" });
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handleRevertCommit(sha);
  } finally {
    uiStore.setLoading(false);
  }
};

const handlePush = async () => {
  try {
    uiStore.setLoading(true, "Pushing changes to remote...", true);
    uiStore.clearError();
    await gitService.push();
    toast.success("Pushed successfully!", { title: "Success" });
    await repoStore.refreshRepo();
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handlePush();
    trackedSetTimeout(() => refreshRepo(), 500);
  } finally {
    uiStore.setLoading(false);
  }
};

const handlePull = async () => {
  try {
    uiStore.setLoading(true, "Pulling from remote...", true);
    uiStore.clearError();
    await gitService.pull();
    toast.success("Pulled successfully!", { title: "Success" });
    await repoStore.refreshRepo();
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handlePull();
    trackedSetTimeout(() => refreshRepo(), 500);
  } finally {
    uiStore.setLoading(false);
  }
};

const handleFetch = async () => {
  try {
    uiStore.setLoading(true, "Fetching from remote...", false);
    uiStore.clearError();
    await gitService.fetch();
    toast.success("Fetch completed!", { title: "Success" });
    await repoStore.refreshRepo();
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handleFetch();
    trackedSetTimeout(() => refreshRepo(), 500);
  } finally {
    uiStore.setLoading(false);
  }
};

const handleStashSave = async () => {
  const message = prompt("Optional stash message:");
  try {
    uiStore.setLoading(true, "Saving stash...", false);
    await gitService.stashSave(message || undefined);
    repoStore.selectedFile = null;
    await repoStore.refreshRepo();
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handleStashSave();
  } finally {
    uiStore.setLoading(false);
  }
};

const handleStashPop = async (index: number) => {
  try {
    uiStore.setLoading(true, "Popping stash...", false);
    uiStore.clearError();
    await gitService.stashPop(index);
    await repoStore.refreshRepo();
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handleStashPop(index);
  } finally {
    uiStore.setLoading(false);
  }
};

const handleApplyStash = async (index: number) => {
  try {
    uiStore.setLoading(true, "Applying stash...", false);
    uiStore.clearError();
    await gitService.applyStash(index);
    await repoStore.refreshRepo();
    toast.success("Stash applied successfully", { title: 'Success' });
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handleApplyStash(index);
  } finally {
    uiStore.setLoading(false);
  }
};

const handleDropStash = async (index: number) => {
  const confirmed = await ask("Are you sure you want to drop this stash? This cannot be undone.", {
    title: 'Drop Stash',
    kind: 'warning'
  });
  if (!confirmed) return;
  try {
    uiStore.setLoading(true, "Dropping stash...", false);
    uiStore.clearError();
    await gitService.dropStash(index);
    await repoStore.refreshRepo();
    toast.success("Stash dropped successfully", { title: 'Success' });
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handleDropStash(index);
  } finally {
    uiStore.setLoading(false);
  }
};

const handleBranchFromStash = async (index: number) => {
  const branchName = prompt("Enter new branch name:");
  if (!branchName || !branchName.trim()) return;
  try {
    uiStore.setLoading(true, "Creating branch from stash...", false);
    uiStore.clearError();
    await gitService.branchFromStash(index, branchName.trim());
    await repoStore.refreshRepo();
    toast.success(`Branch ${branchName} created from stash`, { title: 'Success' });
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handleBranchFromStash(index);
  } finally {
    uiStore.setLoading(false);
  }
};

const handleResolve = async (path: string, ours: boolean) => {
  try {
    uiStore.setLoading(true, "Resolving conflict...", false);
    uiStore.clearError();
    await gitService.resolveConflict(path, ours);
    await repoStore.refreshRepo();
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handleResolve(path, ours);
  } finally {
    uiStore.setLoading(false);
  }
};

const checkoutBranch = async (branchName: string) => {
  try {
    uiStore.setLoading(true, "Checking out branch...", false);
    uiStore.clearError();
    await gitService.checkoutBranch(branchName);
    uiStore.closeModal('branch');
    repoStore.clearSelection();
    await repoStore.refreshRepo();
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await checkoutBranch(branchName);
  } finally {
    uiStore.setLoading(false);
  }
};

const handleCreateBranch = async () => {
  if (!uiStore.newBranchName.trim()) return;
  try {
    uiStore.setLoading(true, "Creating branch...", false);
    await gitService.createBranch(uiStore.newBranchName.trim());
    uiStore.setNewBranchName("");
    uiStore.closeModal('branch');
    await repoStore.refreshRepo();
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handleCreateBranch();
  } finally {
    uiStore.setLoading(false);
  }
};

const handleSwitchToSSH = async () => {
  if (!repoStore.repoInfo) return;
  try {
    uiStore.setLoading(true, "Switching remote to SSH...", false);
    uiStore.clearError();
    
    const currentUrl = await gitService.getRemoteUrl("origin");
    
    if (!currentUrl) {
      toast.error("No remote 'origin' found", { title: "Error" });
      return;
    }
    
    let ownerRepo = "";
    let sshUrl = "";
    
    if (currentUrl.startsWith("https://")) {
      const match = currentUrl.match(/github\.com\/([^\/]+\/[^\/]+)/);
      if (match) {
        ownerRepo = match[1].replace(/\.git$/, '');
      }
      sshUrl = `git@github.com:${ownerRepo}.git`;
    } else if (currentUrl.startsWith("git@")) {
      toast.info("Remote is already using SSH protocol", { title: "Info" });
      uiStore.setLoading(false);
      return;
    } else {
      toast.error("Unsupported remote URL format", { title: "Error" });
      uiStore.setLoading(false);
      return;
    }
    
    if (!ownerRepo) {
      toast.error("Could not parse repository from remote URL", { title: "Error" });
      uiStore.setLoading(false);
      return;
    }
    
    const confirmed = await ask(`Switch remote protocol to SSH?\nNew URL: ${sshUrl}`, { title: 'Switch Remote', kind: 'warning' });
    if (confirmed) {
      await gitService.setRemoteUrl("origin", sshUrl);
      toast.success("Remote protocol switched to SSH successfully!", { title: "Success" });
      uiStore.closeModal('settings');
    }
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handleSwitchToSSH();
  } finally {
    uiStore.setLoading(false);
  }
};

const saveSettings = async () => {
  if (settingsStore.settings) {
    await settingsStore.saveSettings();
    uiStore.closeModal('settings');
  }
};

const toggleTheme = () => {
  settingsStore.toggleTheme();
};

const handleClickOutside = (event: MouseEvent) => {
  if (uiStore.showRecentRepos && dropdownRef.value && !dropdownRef.value.contains(event.target as Node)) {
    uiStore.closeModal('recentRepos');
  }
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

const handleDeleteTag = async (name: string) => {
  const confirmed = await ask(`Delete tag "${name}"?`, { title: 'Delete Tag', kind: 'warning' });
  if (!confirmed) return;
  try {
    uiStore.setLoading(true, "Deleting tag...");
    await gitService.deleteTag(name);
    await loadTags();
    toast.success(`Tag "${name}" deleted`, { title: 'Success' });
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handleDeleteTag(name);
  } finally {
    uiStore.setLoading(false);
  }
};

const handleAddRemote = async () => {
  if (!uiStore.newRemoteName.trim() || !uiStore.newRemoteUrl.trim()) return;
  const name = uiStore.newRemoteName.trim();
  const url = uiStore.newRemoteUrl.trim();
  try {
    uiStore.setLoading(true, "Adding remote...");
    await gitService.addRemote(name, url);
    uiStore.clearNewRemoteFields();
    await loadRemotes();
    toast.success(`Remote "${name}" added`, { title: 'Success' });
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
  } finally {
    uiStore.setLoading(false);
  }
};

const handleRemoveRemote = async (name: string) => {
  const confirmed = await ask(`Remove remote "${name}"?`, { title: 'Remove Remote', kind: 'warning' });
  if (!confirmed) return;
  try {
    uiStore.setLoading(true, "Removing remote...");
    await gitService.removeRemote(name);
    await loadRemotes();
    toast.success(`Remote "${name}" removed`, { title: 'Success' });
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await handleRemoveRemote(name);
  } finally {
    uiStore.setLoading(false);
  }
};

// Keyboard shortcuts
useKeyboardShortcuts([
  { key: 's', ctrl: true, action: () => uiStore.view === 'changes' && handleCommit(), description: 'Commit staged changes' },
  { key: 'p', ctrl: true, action: () => repoStore.repoInfo && handlePush(), description: 'Push changes' },
  { key: 'P', ctrl: true, action: () => repoStore.repoInfo && handlePull(), description: 'Pull changes' },
  { key: 'f', ctrl: true, action: () => repoStore.repoInfo && handleFetch(), description: 'Fetch from remote' },
  { key: 'b', ctrl: true, action: () => repoStore.repoInfo && (uiStore.openModal('branch')), description: 'Open branch switcher' },
  { key: 'k', ctrl: true, action: () => repoStore.repoInfo && handleStashSave(), description: 'Stash changes' },
  { key: 'Escape', action: () => uiStore.closeAllModals(), description: 'Close modal' },
]);

// Sync error from UI store to local ref for template
watch(() => uiStore.error, (newError) => {
  error.value = newError;
});

</script>

<template>
  <Toast />
  <ContextMenu :visible="isVisible" :position="position" :items="menuItems" @close="hideContextMenu" />
  <div class="app flex flex-col h-screen bg-background text-foreground overflow-hidden font-sans">
    <!-- Header/Top Bar -->
    <header class="h-12 border-b border-border flex items-center px-4 justify-between flex-shrink-0 relative top-accent-border" style="background: var(--header-bg);">
      <div class="flex items-center gap-6 text-sm">
        <div ref="dropdownRef" class="relative">
          <div class="flex items-center gap-2 cursor-pointer px-2.5 py-1.5 rounded-lg transition-safe hover:bg-muted" :class="{ 'bg-muted': uiStore.showRecentRepos }" @click="uiStore.showRecentRepos = !uiStore.showRecentRepos" style="color: var(--foreground);">
            <!-- Ark Logo -->
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="color: var(--mark); flex-shrink:0;">
              <path d="M12 2L2 7l10 5 10-5-10-5z"/>
              <path d="M2 17l10 5 10-5"/>
              <path d="M2 12l10 5 10-5"/>
            </svg>
            <div class="flex items-center gap-1.5">
              <span class="text-[11px] font-medium" style="color: var(--muted-foreground);">repo</span>
              <span class="font-semibold text-[13px]" style="color: var(--foreground);">{{ repoStore.repoInfo ? currentProjectName : 'None' }}</span>
            </div>
            <div v-if="repoStore.repoInfo && (repoStore.repoInfo.ahead > 0 || repoStore.repoInfo.behind > 0)" class="flex items-center gap-1 ml-1">
              <span v-if="repoStore.repoInfo.ahead > 0" class="badge text-[10px] font-bold" style="background: var(--success-bg); color: var(--success);">↑{{ repoStore.repoInfo.ahead }}</span>
              <span v-if="repoStore.repoInfo.behind > 0" class="badge text-[10px] font-bold" style="background: var(--error-bg); color: var(--error);">↓{{ repoStore.repoInfo.behind }}</span>
            </div>
            <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" class="text-muted-foreground transition-transform duration-200" :class="{ 'rotate-180': uiStore.showRecentRepos }">
              <polyline points="6 9 12 15 18 9"/>
            </svg>
          </div>

          <!-- Recent Repositories Dropdown -->
          <div v-if="uiStore.showRecentRepos"
               class="absolute top-full left-0 mt-2 w-80 rounded-xl z-50 overflow-hidden py-1.5 glass shadow-xl"
               :style="{ border: '1px solid var(--border)', animation: 'slide-down 0.2s cubic-bezier(0.16,1,0.3,1)' }">
            <div class="px-4 py-2 border-b flex justify-between items-center" style="border-color: var(--border);">
              <span class="text-[10px] font-bold uppercase tracking-widest" style="color: var(--muted-foreground);">Recent Repositories</span>
              <button @click="handleOpenRepo()" class="text-[10px] font-bold transition-safe hover:underline" style="color: var(--accent);">OPEN NEW</button>
            </div>
            <div class="max-h-64 overflow-y-auto">
              <div v-for="path in settingsStore.settings?.recent_repositories" :key="path"
                   @click="handleOpenRepo(path)"
                   class="px-4 py-2.5 cursor-pointer transition-safe flex flex-col gap-0.5 hover:bg-muted"
                   :class="{ 'bg-spotlight': repoStore.repoInfo?.path === path }">
                <div class="text-[13px] font-semibold truncate flex items-center justify-between gap-2">
                  <div class="flex items-center gap-2 truncate">
                    <span v-if="repoStore.repoInfo?.path === path" class="w-1.5 h-1.5 rounded-full flex-shrink-0" style="background: var(--accent);"></span>
                    {{ getRepoName(path) }}
                  </div>
                  <div v-if="repoStore.getRecentRepoInfo(path)" class="flex items-center gap-1 flex-shrink-0">
                    <span v-if="repoStore.getRecentRepoInfo(path)?.ahead" class="badge text-[9px] font-bold" style="background:var(--success-bg);color:var(--success);">↑{{ repoStore.getRecentRepoInfo(path)?.ahead }}</span>
                    <span v-if="repoStore.getRecentRepoInfo(path)?.behind" class="badge text-[9px] font-bold" style="background:var(--error-bg);color:var(--error);">↓{{ repoStore.getRecentRepoInfo(path)?.behind }}</span>
                    <span v-if="repoStore.getRecentRepoInfo(path)?.is_dirty" class="w-1.5 h-1.5 rounded-full" style="background:var(--warning);"></span>
                  </div>
                </div>
                <div class="text-[10px] font-mono truncate" style="color: var(--muted-foreground);">{{ path }}</div>
              </div>
            </div>
            <div v-if="!settingsStore.settings?.recent_repositories.length" class="px-4 py-4 text-center text-xs italic" style="color: var(--muted-foreground);">No recent repositories</div>
          </div>
        </div>

        <div v-if="repoStore.repoInfo" class="flex items-center gap-2 cursor-pointer px-2.5 py-1.5 rounded-lg transition-safe hover:bg-muted" @click="uiStore.openModal('branch')" style="color: var(--foreground);">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="color: var(--muted-foreground);">
            <line x1="6" y1="3" x2="6" y2="15"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/>
            <path d="M18 9a9 9 0 0 1-9 9"/>
          </svg>
          <span class="font-medium text-[13px]" style="color: var(--foreground);">{{ repoStore.getCurrentBranch() }}</span>
        </div>
      </div>

      <!-- Center title -->
      <div v-if="repoStore.repoInfo" class="absolute left-1/2 -translate-x-1/2 hidden md:flex items-center gap-2 pointer-events-none">
        <span class="text-[11px] font-semibold uppercase tracking-widest" style="color: var(--muted-foreground);">{{ currentProjectName }}</span>
      </div>

      <!-- Right Actions -->
      <div class="flex items-center gap-1.5">
        <button v-if="repoStore.repoInfo" @click="triggerCloneModal" class="btn btn-ghost h-8 px-3 text-[13px]">Clone</button>
        <button v-if="repoStore.repoInfo" @click="handleFetch" class="btn btn-ghost h-8 px-3 text-[13px]">Fetch</button>
        <button v-if="repoStore.repoInfo" @click="() => { loadTags(); uiStore.openModal('tags'); }" class="btn btn-ghost h-8 px-3 text-[13px]">Tags</button>
        <button v-if="repoStore.repoInfo" @click="() => { loadRemotes(); uiStore.openModal('remotes'); }" class="btn btn-ghost h-8 px-3 text-[13px]">Remotes</button>
        <div class="w-px h-5 mx-1" style="background: var(--border);"></div>
        <button @click="uiStore.openModal('settings')" class="btn btn-ghost h-8 w-8 p-0" title="Settings" style="color: var(--foreground);">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>
          </svg>
        </button>
        <button @click="toggleTheme" class="btn btn-ghost h-8 w-8 p-0" :title="settingsStore.settings?.theme === 'dark' ? 'Light Mode' : 'Dark Mode'" style="color: var(--foreground);">
          <svg v-if="settingsStore.settings?.theme === 'dark'" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="5"/><line x1="12" y1="1" x2="12" y2="3"/><line x1="12" y1="21" x2="12" y2="23"/><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/><line x1="1" y1="12" x2="3" y2="12"/><line x1="21" y1="12" x2="23" y2="12"/><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/>
          </svg>
          <svg v-else width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/>
          </svg>
        </button>
      </div>
    </header>

    <div v-if="error" class="border-b px-4 py-2.5 text-[13px] flex justify-between items-center" style="background: var(--error-bg); border-color: rgba(248,81,73,0.2); color: var(--error);">
      <span class="font-medium truncate mr-3">{{ error }}</span>
      <div class="flex gap-2">
        <button v-if="uiStore.lastFailedOperation" @click="retryLastOperation" class="btn btn-danger text-xs px-2.5 py-1">Retry</button>
        <button @click="uiStore.clearError" class="btn btn-ghost text-xs px-2 py-1 ml-1">✕</button>
      </div>
    </div>

    <!-- Modals -->
    <div v-if="uiStore.showCloneModal || uiStore.showSettingsModal || uiStore.showBranchModal || uiStore.showTagsModal || uiStore.showRemotesModal" class="fixed inset-0 flex items-center justify-center z-[100] p-4" style="background: rgba(0,0,0,0.65); backdrop-filter: blur(8px);">
      <!-- Clone Modal -->
      <div v-if="uiStore.showCloneModal" class="bg-card rounded-2xl shadow-xl p-8 w-full max-w-md border border-border">
        <h2 class="text-2xl font-display mb-6 text-foreground">Clone Repository</h2>
        
        <div class="mb-5">
          <label class="block mb-2 text-sm font-medium text-foreground">Remote URL</label>
          <input v-model="uiStore.cloneUrl" placeholder="https://github.com/user/repo.git" class="w-full border border-border rounded-lg p-3 text-foreground text-sm focus:ring-2 focus:ring-accent focus:border-transparent outline-none" />
        </div>

        <div class="mb-8">
          <label class="block mb-2 text-sm font-medium text-foreground">Destination Path</label>
          <div class="flex gap-2">
            <input v-model="uiStore.clonePath" placeholder="/path/to/destination" class="flex-1 border border-border rounded-lg p-3 text-foreground text-sm focus:ring-2 focus:ring-accent focus:border-transparent outline-none" />
            <button @click="handleBrowseClonePath" class="px-4 py-3 border border-border rounded-lg hover:bg-muted transition-safe text-sm font-medium">Browse</button>
          </div>
        </div>

        <div class="flex justify-end gap-3">
          <button @click="uiStore.closeModal('clone')" class="px-6 py-2.5 border border-border rounded-lg hover:bg-muted transition-safe font-medium">Cancel</button>
          <button @click="handleCloneRepo" :disabled="!uiStore.cloneUrl || !uiStore.clonePath" class="gradient-bg text-accent-foreground px-6 py-2.5 rounded-lg disabled:opacity-50 disabled:cursor-not-allowed hover:shadow-accent transition-safe font-semibold">Clone</button>
        </div>
      </div>

      <!-- Settings Modal -->
      <div v-if="uiStore.showSettingsModal && settingsStore.settings" class="bg-card rounded-2xl shadow-xl p-8 w-full max-w-md border border-border">
        <h2 class="text-2xl font-display mb-6 text-foreground">Settings</h2>
        <div class="space-y-5 mb-8">
          <div>
            <label class="block text-sm font-semibold text-foreground mb-1">Git User Name</label>
            <p class="text-[11px] text-muted-foreground mb-2 leading-tight">Identifies you as the author of commits</p>
            <input v-model="settingsStore.settings.user_name" class="w-full border border-border rounded-lg p-3 text-foreground text-sm outline-none focus:ring-2 focus:ring-accent focus:border-transparent bg-white shadow-sm" />
          </div>
          <div>
            <label class="block text-sm font-semibold text-foreground mb-1">Git User Email</label>
            <p class="text-[11px] text-muted-foreground mb-2 leading-tight">Email address associated with your commits</p>
            <input v-model="settingsStore.settings.user_email" class="w-full border border-border rounded-lg p-3 text-foreground text-sm outline-none focus:ring-2 focus:ring-accent focus:border-transparent bg-white shadow-sm" />
          </div>
          <div>
            <label class="block text-sm font-semibold text-foreground mb-1">SSH Key Path</label>
            <input v-model="settingsStore.settings.ssh_key_path" placeholder="~/.ssh/id_rsa" class="w-full border border-border rounded-lg p-3 text-foreground text-sm outline-none focus:ring-2 focus:ring-accent focus:border-transparent font-mono bg-white shadow-sm" />
          </div>
          <div class="pt-4 border-t border-border">
            <button @click="handleSwitchToSSH" class="text-sm text-accent hover:underline font-semibold flex items-center gap-2">
              <span>⚠️</span> Switch remotes to SSH
            </button>
            <p class="text-[11px] text-muted-foreground mt-1 leading-tight">Use this if you get authentication errors with HTTPS</p>
          </div>
        </div>
        <div class="flex justify-end gap-3">
          <button @click="uiStore.closeModal('settings')" class="px-6 py-2.5 border border-border rounded-lg hover:bg-muted transition-safe font-medium">Cancel</button>
          <button @click="saveSettings" class="gradient-bg text-accent-foreground px-6 py-2.5 rounded-lg hover:shadow-accent transition-safe font-semibold">Save</button>
        </div>
      </div>

      <!-- Branch Switcher Modal -->
      <div v-if="uiStore.showBranchModal" class="bg-card rounded-2xl shadow-xl p-8 w-full max-w-md border border-border">
        <h2 class="text-2xl font-display mb-6 text-foreground">Branches</h2>
        <div class="max-h-60 overflow-auto mb-6 space-y-2">
          <div v-for="branch in repoStore.branches" :key="branch.name"
               @click="!branch.is_current && checkoutBranch(branch.name)"
               class="p-3 rounded-lg border border-transparent hover:border-border cursor-pointer flex items-center justify-between text-sm transition-safe"
               :class="{ 'gradient-bg text-accent-foreground border-accent shadow-accent': branch.is_current, 'hover:bg-muted': !branch.is_current }">
            <span class="font-medium">{{ branch.name }}</span>
            <span v-if="branch.is_current" class="text-xs font-semibold">Active</span>
          </div>
        </div>
        <div class="border-t border-border pt-6">
          <label class="block text-sm font-medium text-foreground mb-2">Create New Branch</label>
          <div class="flex gap-2">
            <input v-model="uiStore.newBranchName" @keyup.enter="handleCreateBranch" placeholder="feature/new-branch" class="flex-1 border border-border rounded-lg p-3 text-foreground text-sm outline-none focus:ring-2 focus:ring-accent focus:border-transparent font-mono" />
            <button @click="handleCreateBranch" class="gradient-bg text-accent-foreground px-5 rounded-lg hover:shadow-accent transition-safe font-semibold">+</button>
          </div>
        </div>
        <div class="flex justify-end mt-6">
          <button @click="uiStore.closeModal('branch')" class="px-6 py-2.5 border border-border rounded-lg hover:bg-muted transition-safe font-medium">Close</button>
        </div>
      </div>

      <!-- Tags Modal -->
      <div v-if="uiStore.showTagsModal" class="bg-card rounded-2xl shadow-xl p-8 w-full max-w-md border border-border">
        <h2 class="text-2xl font-display mb-6 text-foreground">Tags</h2>
        <div class="max-h-72 overflow-auto mb-6 space-y-2">
          <div v-if="uiStore.tags.length === 0" class="text-center text-muted-foreground text-sm py-8">
            No tags found in this repository
          </div>
          <div v-for="tag in uiStore.tags" :key="tag.name"
               class="p-3 rounded-lg border border-border hover:border-accent cursor-pointer flex items-center justify-between group transition-safe">
            <div class="flex-1 min-w-0">
              <div class="text-sm font-semibold truncate flex items-center gap-2">
                <span class="text-accent">&#9878;</span>
                {{ tag.name }}
              </div>
              <div class="text-xs text-muted-foreground font-mono mt-1">
                {{ tag.sha.substring(0, 7) }}
                <span v-if="tag.date" class="ml-2">{{ new Date(tag.date * 1000).toLocaleDateString() }}</span>
              </div>
              <div v-if="tag.message" class="text-[10px] text-muted-foreground mt-1 truncate">{{ tag.message }}</div>
            </div>
            <button @click="handleDeleteTag(tag.name)" class="opacity-0 group-hover:opacity-100 text-error hover:bg-error/10 px-3 py-1.5 rounded text-xs transition-opacity font-medium">
              Delete
            </button>
          </div>
        </div>
        <div class="flex justify-end mt-6">
          <button @click="uiStore.closeModal('tags')" class="px-6 py-2.5 border border-border rounded-lg hover:bg-muted transition-safe font-medium">Close</button>
        </div>
      </div>

      <!-- Remotes Modal -->
      <div v-if="uiStore.showRemotesModal" class="bg-card rounded-2xl shadow-xl p-8 w-full max-w-md border border-border">
        <h2 class="text-2xl font-display mb-6 text-foreground">Remotes</h2>
        <div class="max-h-72 overflow-auto mb-6 space-y-2">
          <div v-if="uiStore.remotes.length === 0" class="text-center text-muted-foreground text-sm py-8">
            No remotes configured
          </div>
          <div v-for="remote in uiStore.remotes" :key="remote.name"
               class="p-3 rounded-lg border border-border hover:border-accent flex items-center justify-between group transition-safe">
            <div class="flex-1 min-w-0 mr-3">
              <div class="text-sm font-semibold flex items-center gap-2">
                <span class="text-accent">&#9729;</span>
                {{ remote.name }}
              </div>
              <div class="text-xs text-muted-foreground font-mono mt-1 truncate" :title="remote.url">{{ remote.url }}</div>
            </div>
            <button @click="handleRemoveRemote(remote.name)" class="opacity-0 group-hover:opacity-100 text-error hover:bg-error/10 px-3 py-1.5 rounded text-xs transition-opacity font-medium">
              Remove
            </button>
          </div>
        </div>
        <div class="border-t border-border pt-6">
          <label class="block text-sm font-medium text-foreground mb-2">Add Remote</label>
          <div class="space-y-3">
            <input v-model="uiStore.newRemoteName" placeholder="e.g. upstream" class="w-full border border-border rounded-lg p-3 text-foreground text-sm outline-none focus:ring-2 focus:ring-accent focus:border-transparent" />
            <input v-model="uiStore.newRemoteUrl" placeholder="https://github.com/user/repo.git" class="w-full border border-border rounded-lg p-3 text-foreground text-sm outline-none focus:ring-2 focus:ring-accent focus:border-transparent font-mono" />
            <button @click="handleAddRemote" :disabled="!uiStore.newRemoteName.trim() || !uiStore.newRemoteUrl.trim()" class="w-full gradient-bg text-accent-foreground disabled:opacity-50 disabled:cursor-not-allowed px-5 py-2.5 rounded-lg hover:shadow-accent transition-safe font-semibold">
              Add Remote
            </button>
          </div>
        </div>
        <div class="flex justify-end mt-6">
          <button @click="uiStore.closeModal('remotes')" class="px-6 py-2.5 border border-border rounded-lg hover:bg-muted transition-safe font-medium">Close</button>
        </div>
      </div>
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
          <div v-if="uiStore.view === 'changes'" class="space-y-1.5">
            <!-- Changes Header with Bulk Select -->
            <div v-if="repoStore.fileStatuses.length > 0" 
                 @contextmenu.prevent="onFileHeaderContextMenu"
                 class="flex items-center gap-3 p-2.5 mb-2 rounded-lg bg-muted/50 border border-border transition-safe justify-between">
              <div class="flex items-center gap-3 cursor-pointer" @click="toggleAllStaged">
                <input type="checkbox" :checked="repoStore.allStaged" class="w-4 h-4 rounded border-border accent-accent cursor-pointer pointer-events-none" />
                <div class="text-xs font-semibold text-muted-foreground select-none">
                  {{ repoStore.fileStatuses.length }} changed file{{ repoStore.fileStatuses.length !== 1 ? 's' : '' }}
                </div>
              </div>
              <button @click.stop="handleDiscardAllChanges" class="text-[10px] text-error hover:underline font-bold px-2 py-1 rounded hover:bg-error/10 transition-safe">DISCARD ALL</button>
            </div>

            <div v-for="file in repoStore.fileStatuses" :key="file.path"
                 @contextmenu.prevent="onFileContextMenu($event, file)"
                 class="group flex items-center gap-2.5 px-2.5 py-2 rounded-lg cursor-pointer transition-safe"
                 :style="repoStore.selectedFile === file.path ? 'background:var(--spotlight); border:1px solid var(--accent); border-opacity:0.3;' : 'border:1px solid transparent;'"
                 :class="{ 'hover:bg-muted': repoStore.selectedFile !== file.path }"
                 @click.self="repoStore.selectedFile = file.path">
              <input type="checkbox" :checked="file.staged" @change="toggleStaged(file)" class="w-3.5 h-3.5 rounded flex-shrink-0" style="accent-color: var(--accent);" />
              <div class="flex-1 min-w-0 flex items-center gap-2" @click="repoStore.selectedFile = file.path">
                <span class="text-[10px] w-4 text-center font-bold flex-shrink-0"
                  :style="file.status === 'added' ? 'color:var(--success)' : file.status === 'deleted' ? 'color:var(--error)' : 'color:var(--accent)'">
                  {{ file.status[0].toUpperCase() }}
                </span>
                <span class="truncate text-[13px]" :title="file.path">{{ file.path.split('/').pop() }}</span>
                <span class="text-[10px] font-mono truncate flex-shrink-0" style="color:var(--muted-foreground); max-width:60px;">{{ file.path.includes('/') ? file.path.substring(0, file.path.lastIndexOf('/')) : '' }}</span>
              </div>
              <button @click.stop="handleDiscardChanges(file.path)" class="opacity-0 group-hover:opacity-100 w-5 h-5 flex items-center justify-center rounded transition-safe flex-shrink-0 text-[10px]" style="color:var(--error);" onmouseover="this.style.background='var(--error-bg)'" onmouseout="this.style.background=''">✕</button>
            </div>
          </div>
          <div v-else-if="uiStore.view === 'history'" class="flex-1 flex flex-col overflow-hidden">
            <div class="px-3 py-2 border-b border-border bg-card/50">
               <input v-model="uiStore.searchCommitQuery" placeholder="Search commits..." class="w-full bg-muted/30 border border-border rounded-lg px-3 py-2 text-xs text-foreground outline-none focus:ring-1 focus:ring-accent" />
            </div>
            <RecycleScroller
              class="flex-1 overflow-auto p-3"
              :items="filteredCommits"
              :item-size="76"
              key-field="sha"
              v-slot="{ item }"
            >
              <div @click="repoStore.selectedCommit = item"
                   @contextmenu.prevent="onCommitContextMenu($event, item)"
                   class="mb-1.5 p-3 rounded-lg border border-transparent hover:border-border cursor-pointer transition-safe bg-card/30"
                   :class="{ 'border-accent bg-accent/5 shadow-sm': repoStore.selectedCommit?.sha === item.sha }">
                <div class="text-sm font-semibold truncate mb-1.5 flex items-center gap-2" :class="{ 'text-accent': repoStore.selectedCommit?.sha === item.sha }">
                  <span v-if="!item.is_pushed" 
                        class="text-success font-bold text-xs" title="Unpushed commit">↑</span>
                  {{ item.message }}
                </div>
                <div class="flex justify-between text-xs text-muted-foreground font-mono">
                  <span>{{ item.sha.substring(0, 7) }}</span>
                  <span>{{ new Date(item.timestamp * 1000).toLocaleDateString() }}</span>
                </div>
              </div>
            </RecycleScroller>
          </div>
          <div v-else-if="uiStore.view === 'stashes'" class="space-y-1.5">
            <div v-for="(stash, index) in repoStore.stashes" :key="index"
                 @contextmenu.prevent="onStashContextMenu($event, stash)"
                 class="p-3 bg-card rounded-lg border border-border flex justify-between items-center group hover:border-accent transition-safe">
              <div class="flex-1 min-w-0">
                <div class="text-sm font-semibold truncate">{{ stash.message || 'No message' }}</div>
                <div class="text-xs text-muted-foreground font-mono mt-1">{{ stash.sha.substring(0, 7) }}</div>
              </div>
              <button @click="handleStashPop(index)" class="opacity-0 group-hover:opacity-100 gradient-bg text-accent-foreground text-xs px-3 py-1.5 rounded-lg hover:shadow-accent transition-safe font-medium">Pop</button>
            </div>
          </div>
          <div v-else-if="uiStore.view === 'conflicts'" class="space-y-2">
            <div v-for="conflict in repoStore.conflicts" :key="conflict.path"
                 @contextmenu.prevent="onConflictContextMenu($event, conflict)"
                 class="p-3 bg-error/5 rounded-lg border border-error/20">
              <div class="text-sm font-semibold truncate mb-3 text-error" :title="conflict.path">{{ conflict.path.split('/').pop() }}</div>
              <div class="flex gap-2 text-xs">
                <button @click="handleResolve(conflict.path, true)" class="flex-1 bg-card border border-border hover:bg-muted py-2 rounded-lg font-medium transition-safe">Use Ours</button>
                <button @click="handleResolve(conflict.path, false)" class="flex-1 bg-card border border-border hover:bg-muted py-2 rounded-lg font-medium transition-safe">Use Theirs</button>
              </div>
            </div>
          </div>
        </div>

        <div v-if="uiStore.view === 'changes'" class="p-4 border-t border-border bg-muted/30">
          <div class="flex items-center gap-2 mb-2">
             <input type="checkbox" id="amend" v-model="uiStore.amendCommit" class="w-3.5 h-3.5 rounded border-border accent-accent" />
             <label for="amend" class="text-xs font-medium text-muted-foreground cursor-pointer select-none">Amend Last Commit</label>
          </div>
          <label class="block mb-2 text-sm font-medium text-foreground">Commit Message</label>
          <textarea v-model="uiStore.commitMessage" placeholder="Describe your changes..." class="w-full bg-card border border-border rounded-lg p-3 text-foreground text-sm mb-3 focus:ring-2 focus:ring-accent focus:border-transparent outline-none resize-none" rows="3" />
          <button @click="handleCommit" :disabled="uiStore.loading || !uiStore.commitMessage.trim() || (!uiStore.amendCommit && repoStore.stagedFiles.length === 0)" 
                  class="w-full gradient-bg text-accent-foreground disabled:opacity-50 disabled:cursor-not-allowed py-2.5 rounded-lg font-semibold text-sm hover:shadow-accent transition-safe">
            {{ uiStore.amendCommit ? 'Amend Commit' : `Commit to ${repoStore.getCurrentBranch()}` }}
          </button>
        </div>

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
            <DiffViewer :diffs="repoStore.diffs" />
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
              <DiffViewer :diffs="repoStore.diffs.filter(d => d.path === repoStore.selectedCommitFile)" />
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
          <button @click="handleOpenRepo()" class="group p-5 rounded-xl border text-left transition-safe"
            style="background: var(--card); border-color: var(--border);"
            onmouseover="this.style.background='var(--muted)'"
            onmouseout="this.style.background='var(--card)'">
            <div class="w-8 h-8 rounded-lg flex items-center justify-center mb-3" style="background: var(--muted-foreground); opacity:0.15;">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="stroke: var(--foreground); opacity:1;">
                <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
              </svg>
            </div>
            <div class="font-medium text-[14px] mb-0.5" style="color: var(--foreground);">Open</div>
            <div class="text-[12px]" style="color: var(--muted-foreground);">Load from filesystem</div>
          </button>
          <button @click="triggerCloneModal" class="group p-5 rounded-xl border text-left transition-safe"
            style="background: var(--card); border-color: var(--border);"
            onmouseover="this.style.background='var(--muted)'"
            onmouseout="this.style.background='var(--card)'">
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
                 class="group flex items-center gap-3 px-4 py-3 rounded-xl border cursor-pointer transition-safe"
                 style="background: var(--card); border-color: var(--border);"
                 onmouseover="this.style.borderColor='var(--accent)'"
                 onmouseout="this.style.borderColor='var(--border)'">
              <div class="w-7 h-7 rounded-lg flex items-center justify-center flex-shrink-0 text-[11px] font-bold" style="background:var(--muted); color:var(--accent);">{{ getRepoName(path)[0]?.toUpperCase() }}</div>
              <div class="flex-1 min-w-0">
                <div class="font-semibold truncate text-[13px]">{{ getRepoName(path) }}</div>
                <div class="text-[11px] font-mono truncate" style="color:var(--muted-foreground);">{{ path }}</div>
              </div>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="flex-shrink-0 transition-safe" style="color:var(--muted-foreground);" onmouseover="this.style.color='var(--accent)'">
                <polyline points="9 18 15 12 9 6"/>
              </svg>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Loading Overlay (major operations) -->
    <div v-if="uiStore.loading && uiStore.isMajorOperation" class="fixed inset-0 z-[100] flex items-center justify-center" style="background: rgba(0,0,0,0.6); backdrop-filter: blur(8px);">
      <div class="rounded-2xl p-8 flex flex-col items-center gap-5" style="background:var(--card); border:1px solid var(--border); box-shadow:var(--shadow-xl);">
        <div class="spinner spinner-lg"></div>
        <span class="text-[15px] font-semibold">{{ uiStore.loadingMessage || 'Processing...' }}</span>
      </div>
    </div>
    <!-- Small loading indicator -->
    <div v-else-if="uiStore.loading" class="fixed bottom-4 right-4 z-[100] flex items-center gap-2 px-3 py-2 rounded-xl" style="background:var(--card); border:1px solid var(--border); box-shadow:var(--shadow-lg); backdrop-filter:blur(12px);">
      <div class="spinner"></div>
      <span class="text-[12px] font-medium" style="color:var(--muted-foreground);">{{ uiStore.loadingMessage || 'Loading...' }}</span>
    </div>
  </div>
</template>

<style>
/* Minimalist modern styles defined in index.css */
</style>
