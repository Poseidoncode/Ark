<script setup lang="ts">
import { DynamicScroller, DynamicScrollerItem } from 'vue-virtual-scroller';
import 'vue-virtual-scroller/dist/vue-virtual-scroller.css';
import { useRepoStore } from '../stores/repo';
import { useUIStore } from '../stores/ui';
import { gitService, type StashInfo } from '../services/git';
import { useToast } from '../composables/useToast';
import { useContextMenu } from '../composables/useContextMenu';
import { useRepoLock } from '../composables/useOperationMutex';

const repoStore = useRepoStore();
const uiStore = useUIStore();
const toast = useToast();
const { showContextMenu } = useContextMenu();
const { withRepoLock } = useRepoLock();

const emit = defineEmits<{
  (e: 'requestStashBranch', stash: StashInfo): void;
}>();

const handleStashPop = async (stash: StashInfo) => {
  let mutated = false;
  try {
    uiStore.setLoading(true, "Popping stash...", false);
    uiStore.clearError();
    await withRepoLock('stash-pop', repoStore.repoInfo?.path, async () => {
      await gitService.stashPop(stash.sha);
    });
    mutated = true;
    await repoStore.refreshRepo();
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = mutated
      ? async () => { await repoStore.refreshRepo(); }
      : async () => await handleStashPop(stash);
  } finally {
    uiStore.setLoading(false);
  }
};

const handleStashApply = async (stash: StashInfo) => {
  let mutated = false;
  try {
    uiStore.setLoading(true, "Applying stash...", false);
    uiStore.clearError();
    await withRepoLock('stash-apply', repoStore.repoInfo?.path, async () => {
      await gitService.applyStash(stash.sha);
    });
    mutated = true;
    await repoStore.refreshRepo();
    toast.success("Stash applied successfully", { title: 'Success' });
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = mutated
      ? async () => { await repoStore.refreshRepo(); }
      : async () => await handleStashApply(stash);
  } finally {
    uiStore.setLoading(false);
  }
};

const handleStashDrop = async (stash: StashInfo) => {
  const { ask } = await import('@tauri-apps/plugin-dialog');
  const confirmed = await ask("Are you sure you want to drop this stash? This cannot be undone.", {
    title: 'Drop Stash',
    kind: 'warning'
  });
  if (!confirmed) return;
  let mutated = false;
  try {
    uiStore.setLoading(true, "Dropping stash...", false);
    uiStore.clearError();
    await withRepoLock('stash-drop', repoStore.repoInfo?.path, async () => {
      await gitService.dropStash(stash.sha);
    });
    mutated = true;
    await repoStore.refreshRepo();
    toast.success("Stash dropped successfully", { title: 'Success' });
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = mutated
      ? async () => { await repoStore.refreshRepo(); }
      : async () => await handleStashDrop(stash);
  } finally {
    uiStore.setLoading(false);
  }
};

const onStashContextMenu = (event: MouseEvent, stash: StashInfo) => {
  showContextMenu(event, [
    {
      label: 'Apply Stash',
      action: () => handleStashApply(stash)
    },
    {
      label: 'Pop Stash',
      action: () => handleStashPop(stash)
    },
    {
      label: 'Drop Stash',
      danger: true,
      action: () => handleStashDrop(stash)
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
  <DynamicScroller
    :items="repoStore.stashes"
    :min-item-size="66"
    key-field="sha"
    class="flex-1 overflow-auto"
  >
    <template #default="{ item, index, active }">
      <DynamicScrollerItem :item="item" :active="active" :data-index="index">
        <div class="pb-1.5">
          <div @contextmenu.prevent="onStashContextMenu($event, item)"
               class="p-3 bg-card rounded-lg border border-border flex justify-between items-center group hover:border-accent transition-safe">
            <div class="flex-1 min-w-0">
              <div class="text-sm font-semibold truncate">{{ item.message || 'No message' }}</div>
              <div class="text-xs text-muted-foreground font-mono mt-1">{{ item.sha.substring(0, 7) }}</div>
            </div>
            <button @click="handleStashPop(item)" class="opacity-0 group-hover:opacity-100 gradient-bg text-accent-foreground text-xs px-3 py-1.5 rounded-lg hover:shadow-accent transition-safe font-medium">Pop</button>
          </div>
        </div>
      </DynamicScrollerItem>
    </template>
  </DynamicScroller>
</template>
