<script setup lang="ts">
import { ref, computed, watch, onUnmounted } from 'vue';
import { RecycleScroller } from 'vue-virtual-scroller';
import 'vue-virtual-scroller/dist/vue-virtual-scroller.css';
import { useRepoStore } from '../stores/repo';
import { useUIStore } from '../stores/ui';
import { gitService, type CommitInfo } from '../services/git';
import { useToast } from '../composables/useToast';
import { useContextMenu } from '../composables/useContextMenu';
import { useRepoLock } from '../composables/useOperationMutex';
import { openUrl } from '@tauri-apps/plugin-opener';
import { ask } from '@tauri-apps/plugin-dialog';

const repoStore = useRepoStore();
const uiStore = useUIStore();
const toast = useToast();
const { showContextMenu } = useContextMenu();
const { withRepoLock } = useRepoLock();

const emit = defineEmits<{
  (e: 'requestCreateBranch', commit: CommitInfo): void;
  (e: 'requestCreateTag', commit: CommitInfo): void;
}>();

// Search runs against a prebuilt lowercase index (built once per history
// refresh, not once per keystroke) and is debounced (P7).
interface IndexedCommit extends CommitInfo {
  _search: string;
  _date: string;
}

const indexedCommits = computed<IndexedCommit[]>(() =>
  repoStore.commits.map(c => ({
    ...c,
    _search: `${c.message}\n${c.sha}\n${c.author}`.toLowerCase(),
    _date: new Date(c.timestamp * 1000).toLocaleDateString(),
  }))
);

const rawQuery = ref(uiStore.searchCommitQuery);
let searchTimer: ReturnType<typeof setTimeout> | null = null;
watch(rawQuery, (v) => {
  if (searchTimer) clearTimeout(searchTimer);
  searchTimer = setTimeout(() => {
    searchTimer = null;
    uiStore.setSearchCommitQuery(v);
  }, 200);
});
onUnmounted(() => {
  if (searchTimer) clearTimeout(searchTimer);
});

const filteredCommits = computed(() => {
  const q = uiStore.searchCommitQuery.trim().toLowerCase();
  if (!q) return indexedCommits.value;
  return indexedCommits.value.filter(c => c._search.includes(q));
});

const handleCherryPick = async (commit: CommitInfo) => {
  const confirmed = await ask(`Cherry-pick commit ${commit.sha.substring(0, 7)}?`, { title: 'Cherry-pick', kind: 'info' });
  if (!confirmed) return;
  let mutated = false;
  try {
    uiStore.setLoading(true, "Cherry-picking commit...", false);
    await withRepoLock('cherry-pick', repoStore.repoInfo?.path, async () => {
      await gitService.cherryPick(commit.sha);
    });
    mutated = true;
    await repoStore.refreshRepo();
    toast.success("Cherry-pick successful", { title: "Success" });
    uiStore.clearError();
  } catch (e) {
    uiStore.setError(String(e));
    uiStore.lastFailedOperation = mutated
      ? async () => { await repoStore.refreshRepo(); }
      : async () => await handleCherryPick(commit);
  } finally {
    uiStore.setLoading(false);
  }
};

const handleRevertCommit = async (commit: CommitInfo) => {
  const confirmed = await ask(`Revert commit ${commit.sha.substring(0, 7)}?`, { title: 'Revert Commit', kind: 'warning' });
  if (!confirmed) return;
  let mutated = false;
  try {
    uiStore.setLoading(true, "Reverting commit...", false);
    await withRepoLock('revert', repoStore.repoInfo?.path, async () => {
      await gitService.revertCommit(commit.sha);
    });
    mutated = true;
    await repoStore.refreshRepo();
    toast.success("Revert successful", { title: "Success" });
    uiStore.clearError();
  } catch (e) {
    uiStore.setError(String(e));
    uiStore.lastFailedOperation = mutated
      ? async () => { await repoStore.refreshRepo(); }
      : async () => await handleRevertCommit(commit);
  } finally {
    uiStore.setLoading(false);
  }
};

const handleResetBranch = async (commit: CommitInfo) => {
  const confirmed = await ask(`Reset current branch to ${commit.sha.substring(0, 7)}? This will discard all commits after this point.`, {
    title: 'Reset Branch',
    kind: 'warning'
  });
  if (!confirmed) return;
  let mutated = false;
  try {
    uiStore.setLoading(true, "Resetting branch...", false);
    await withRepoLock('reset', repoStore.repoInfo?.path, async () => {
      await gitService.resetBranch(commit.sha);
    });
    mutated = true;
    await repoStore.refreshRepo();
    toast.success("Branch reset successfully", { title: 'Success' });
    uiStore.clearError();
  } catch (e) {
    uiStore.setError(String(e));
    uiStore.lastFailedOperation = mutated
      ? async () => { await repoStore.refreshRepo(); }
      : async () => await handleResetBranch(commit);
  } finally {
    uiStore.setLoading(false);
  }
};

const handleMergeCommit = async (commit: CommitInfo) => {
  const confirmed = await ask(`Merge commit ${commit.sha.substring(0, 7)} into current branch?`, {
    title: 'Merge Commit',
    kind: 'info'
  });
  if (!confirmed) return;
  let mutated = false;
  try {
    uiStore.setLoading(true, "Merging commit...", false);
    await withRepoLock('merge', repoStore.repoInfo?.path, async () => {
      await gitService.mergeCommit(commit.sha);
    });
    mutated = true;
    await repoStore.refreshRepo();
    toast.success("Merge successful", { title: 'Success' });
    uiStore.clearError();
  } catch (e) {
    uiStore.setError(String(e));
    uiStore.lastFailedOperation = mutated
      ? async () => { await repoStore.refreshRepo(); }
      : async () => await handleMergeCommit(commit);
  } finally {
    uiStore.setLoading(false);
  }
};

const onCommitContextMenu = (event: MouseEvent, commit: CommitInfo) => {
  showContextMenu(event, [
    {
      label: 'Copy SHA',
      action: async () => {
        try {
          await navigator.clipboard.writeText(commit.sha.substring(0, 7));
          toast.success('SHA copied', { title: 'Copied' });
        } catch {
          toast.error('Failed to copy SHA', { title: 'Clipboard Error' });
        }
      }
    },
    {
      label: 'Copy Full SHA',
      action: async () => {
        try {
          await navigator.clipboard.writeText(commit.sha);
          toast.success('Full SHA copied', { title: 'Copied' });
        } catch {
          toast.error('Failed to copy SHA', { title: 'Clipboard Error' });
        }
      }
    },
    {
      label: 'Copy Commit Message',
      action: async () => {
        try {
          await navigator.clipboard.writeText(commit.message);
          toast.success('Message copied', { title: 'Copied' });
        } catch {
          toast.error('Failed to copy message', { title: 'Clipboard Error' });
        }
      }
    },
    { divider: true },
    {
      label: 'Create Branch from Commit',
      action: () => emit('requestCreateBranch', commit)
    },
    {
      label: 'Create Tag',
      action: () => emit('requestCreateTag', commit)
    },
    {
      label: 'Cherry-pick Commit',
      action: () => handleCherryPick(commit)
    },
    {
      label: 'Revert Commit',
      danger: true,
      action: () => handleRevertCommit(commit)
    },
    {
      label: 'Reset Branch to this Commit',
      danger: true,
      action: () => handleResetBranch(commit)
    },
    {
      label: 'Merge into Current Branch',
      action: () => handleMergeCommit(commit)
    },
    { divider: true },
    {
      label: 'View on GitHub',
      action: async () => {
        try {
          const url = await gitService.getRemoteUrl("origin");
          if (url) {
            let httpsUrl = "";
            if (url.startsWith('git@github.com:')) {
               httpsUrl = url.replace('git@github.com:', 'https://github.com/').replace(/\.git$/, '');
            } else if (url.startsWith('https://github.com/') || url.startsWith('http://github.com/')) {
               httpsUrl = url.replace(/\.git$/, '');
            } else {
              toast.error("Remote is not a GitHub URL", { title: "Error" });
              return;
            }
            const parsed = new URL(httpsUrl);
            if (parsed.hostname.toLowerCase() !== 'github.com') {
              toast.error("Remote is not a GitHub URL", { title: "Error" });
              return;
            }
            await openUrl(`${parsed.origin}${parsed.pathname.replace(/\/$/, '')}/commit/${commit.sha}`);
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
</script>

<template>
  <div class="flex-1 flex flex-col overflow-hidden min-h-0">
    <div class="px-3 py-2 border-b border-border bg-card/50 flex-shrink-0">
       <input v-model="rawQuery" placeholder="Search commits..." class="w-full bg-muted/30 border border-border rounded-lg px-3 py-2 text-xs text-foreground outline-none focus:ring-1 focus:ring-accent" />
    </div>
    <div v-show="!repoStore.commitsLoading && filteredCommits.length === 0" class="flex-1 flex items-center justify-center text-muted-foreground text-xs italic p-6">
      No commits found
    </div>
    <RecycleScroller
      v-show="filteredCommits.length > 0"
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
          <span>{{ item._date }}</span>
        </div>
      </div>
    </RecycleScroller>
  </div>
</template>
