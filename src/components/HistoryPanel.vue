<script setup lang="ts">
import { computed } from 'vue';
import { RecycleScroller } from 'vue-virtual-scroller';
import 'vue-virtual-scroller/dist/vue-virtual-scroller.css';
import { useRepoStore } from '../stores/repo';
import { useUIStore } from '../stores/ui';
import { gitService, type CommitInfo } from '../services/git';
import { useToast } from '../composables/useToast';
import { useContextMenu } from '../composables/useContextMenu';
import { openUrl } from '@tauri-apps/plugin-opener';
import { ask } from '@tauri-apps/plugin-dialog';

const repoStore = useRepoStore();
const uiStore = useUIStore();
const toast = useToast();
const { showContextMenu } = useContextMenu();

const emit = defineEmits<{
  (e: 'requestCreateBranch', commit: CommitInfo): void;
  (e: 'requestCreateTag', commit: CommitInfo): void;
}>();

const filteredCommits = computed(() => {
  if (!uiStore.searchCommitQuery.trim()) return repoStore.commits;
  const q = uiStore.searchCommitQuery.toLowerCase();
  return repoStore.commits.filter(c => 
    c.message.toLowerCase().includes(q) || 
    c.sha.toLowerCase().includes(q) || 
    c.author.toLowerCase().includes(q)
  );
});

const handleCherryPick = async (commit: CommitInfo) => {
  const confirmed = await ask(`Cherry-pick commit ${commit.sha.substring(0, 7)}?`, { title: 'Cherry-pick', kind: 'info' });
  if (!confirmed) return;
  try {
    uiStore.setLoading(true, "Cherry-picking commit...", false);
    await gitService.cherryPick(commit.sha);
    await repoStore.refreshRepo();
    toast.success("Cherry-pick successful", { title: "Success" });
    uiStore.clearError();
  } catch (e) {
    uiStore.setError(String(e));
    uiStore.lastFailedOperation = async () => handleCherryPick(commit);
  } finally {
    uiStore.setLoading(false);
  }
};

const handleRevertCommit = async (commit: CommitInfo) => {
  const confirmed = await ask(`Revert commit ${commit.sha.substring(0, 7)}?`, { title: 'Revert Commit', kind: 'warning' });
  if (!confirmed) return;
  try {
    uiStore.setLoading(true, "Reverting commit...", false);
    await gitService.revertCommit(commit.sha);
    await repoStore.refreshRepo();
    toast.success("Revert successful", { title: "Success" });
    uiStore.clearError();
  } catch (e) {
    uiStore.setError(String(e));
    uiStore.lastFailedOperation = async () => handleRevertCommit(commit);
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
</script>

<template>
  <div class="flex-1 flex flex-col overflow-hidden">
    <div class="px-3 py-2 border-b border-border bg-card/50">
       <input v-model="uiStore.searchCommitQuery" placeholder="Search commits..." class="w-full bg-muted/30 border border-border rounded-lg px-3 py-2 text-xs text-foreground outline-none focus:ring-1 focus:ring-accent" />
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
          <span>{{ new Date(item.timestamp * 1000).toLocaleDateString() }}</span>
        </div>
      </div>
    </RecycleScroller>
  </div>
</template>
