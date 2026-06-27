<script setup lang="ts">
import { useRepoStore } from '../stores/repo';
import { useUIStore } from '../stores/ui';
import { gitService, type StashInfo } from '../services/git';
import { useToast } from '../composables/useToast';
import { useContextMenu } from '../composables/useContextMenu';

const repoStore = useRepoStore();
const uiStore = useUIStore();
const toast = useToast();
const { showContextMenu } = useContextMenu();

const emit = defineEmits<{
  (e: 'requestStashBranch', stash: StashInfo): void;
}>();

const handleStashPop = async (stash: StashInfo) => {
  try {
    uiStore.setLoading(true, "Popping stash...", false);
    uiStore.clearError();
    await gitService.stashPop(stash.sha);
    await repoStore.refreshRepo();
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => await gitService.stashPop(stash.sha);
  } finally {
    uiStore.setLoading(false);
  }
};

const onStashContextMenu = (event: MouseEvent, stash: StashInfo) => {
  showContextMenu(event, [
    {
      label: 'Apply Stash',
      action: async () => {
        try {
          uiStore.setLoading(true, "Applying stash...", false);
          uiStore.clearError();
          await gitService.applyStash(stash.sha);
          await repoStore.refreshRepo();
          toast.success("Stash applied successfully", { title: 'Success' });
          uiStore.clearError();
        } catch (err) {
          uiStore.setError(String(err));
          uiStore.lastFailedOperation = async () => await gitService.applyStash(stash.sha);
        } finally {
          uiStore.setLoading(false);
        }
      }
    },
    {
      label: 'Pop Stash',
      action: () => handleStashPop(stash)
    },
    {
      label: 'Drop Stash',
      danger: true,
      action: async () => {
        const { ask } = await import('@tauri-apps/plugin-dialog');
        const confirmed = await ask("Are you sure you want to drop this stash? This cannot be undone.", {
          title: 'Drop Stash',
          kind: 'warning'
        });
        if (!confirmed) return;
        try {
          uiStore.setLoading(true, "Dropping stash...", false);
          uiStore.clearError();
          await gitService.dropStash(stash.sha);
          await repoStore.refreshRepo();
          toast.success("Stash dropped successfully", { title: 'Success' });
          uiStore.clearError();
        } catch (err) {
          uiStore.setError(String(err));
          uiStore.lastFailedOperation = async () => await gitService.dropStash(stash.sha);
        } finally {
          uiStore.setLoading(false);
        }
      }
    },
    { divider: true },
    {
      label: 'Create Branch from Stash',
      action: () => emit('requestStashBranch', stash)
    },
    {
      label: 'Copy SHA',
      action: async () => {
        try {
          await navigator.clipboard.writeText(stash.sha);
          toast.success('SHA copied', { title: 'Copied' });
        } catch {
          toast.error('Failed to copy SHA', { title: 'Clipboard Error' });
        }
      }
    }
  ]);
};
</script>

<template>
  <div class="space-y-1.5">
    <div v-for="(stash, index) in repoStore.stashes" :key="index"
         @contextmenu.prevent="onStashContextMenu($event, stash)"
         class="p-3 bg-card rounded-lg border border-border flex justify-between items-center group hover:border-accent transition-safe">
      <div class="flex-1 min-w-0">
        <div class="text-sm font-semibold truncate">{{ stash.message || 'No message' }}</div>
        <div class="text-xs text-muted-foreground font-mono mt-1">{{ stash.sha.substring(0, 7) }}</div>
      </div>
      <button @click="handleStashPop(stash)" class="opacity-0 group-hover:opacity-100 gradient-bg text-accent-foreground text-xs px-3 py-1.5 rounded-lg hover:shadow-accent transition-safe font-medium">Pop</button>
    </div>
  </div>
</template>
