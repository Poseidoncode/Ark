<script setup lang="ts">
import { useRepoStore } from '../stores/repo';
import { useUIStore } from '../stores/ui';
import { gitService, type ConflictInfo } from '../services/git';
import { useToast } from '../composables/useToast';
import { useContextMenu } from '../composables/useContextMenu';
import { openPath } from '@tauri-apps/plugin-opener';

const repoStore = useRepoStore();
const uiStore = useUIStore();
const toast = useToast();
const { showContextMenu } = useContextMenu();

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
        try {
          await navigator.clipboard.writeText(conflict.path);
          toast.success('Path copied', { title: 'Copied' });
        } catch {
          toast.error('Failed to copy path', { title: 'Clipboard Error' });
        }
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
</script>

<template>
  <div class="space-y-2">
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
</template>
