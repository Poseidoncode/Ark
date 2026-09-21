<script setup lang="ts">
import { DynamicScroller, DynamicScrollerItem } from 'vue-virtual-scroller';
import 'vue-virtual-scroller/dist/vue-virtual-scroller.css';
import { useRepoStore } from '../stores/repo';
import { useUIStore } from '../stores/ui';
import { gitService, type FileStatus, type StageResult } from '../services/git';
import { useToast } from '../composables/useToast';
import { useContextMenu } from '../composables/useContextMenu';
import { useRepoLock } from '../composables/useOperationMutex';
import { openPath } from '@tauri-apps/plugin-opener';

const repoStore = useRepoStore();
const uiStore = useUIStore();
const toast = useToast();
const { showContextMenu } = useContextMenu();
const { withRepoLock } = useRepoLock();

const emit = defineEmits<{
  (e: 'toggleAllStaged'): void;
  (e: 'handleDiscardAllChanges'): void;
  (e: 'contextMenu', event: MouseEvent, file: FileStatus): void;
  (e: 'fileHeaderContextMenu', event: MouseEvent): void;
}>();

const toggleStaged = async (file: FileStatus) => {
  let mutated = false;
  try {
    await withRepoLock('toggle-staged', repoStore.repoInfo?.path, async () => {
      if (file.staged) {
        await gitService.unstageFiles([file.path]);
      } else {
        const result: StageResult = await gitService.stageFiles([file.path]);
        if (result.warnings.length > 0) {
          uiStore.setError(result.warnings.join('\n'));
        }
      }
    });
    mutated = true;
    await repoStore.refreshRepo();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = mutated
      ? async () => { await repoStore.refreshRepo(); }
      : async () => await toggleStaged(file);
  }
};

const handleDiscardChanges = async (path: string) => {
  const { ask } = await import('@tauri-apps/plugin-dialog');
  const confirmed = await ask(`Are you sure you want to discard changes in ${path}? This cannot be undone.`, { 
    title: 'Discard Changes',
    kind: 'warning'
  });
  if (!confirmed) return;
  let mutated = false;
  try {
    uiStore.setLoading(true, "Discarding changes...", false);
    await withRepoLock('discard', repoStore.repoInfo?.path, async () => {
      await gitService.discardChanges(path);
    });
    mutated = true;
    if (repoStore.selectedFile === path) repoStore.selectedFile = null;
    await repoStore.refreshRepo();
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = mutated
      ? async () => { await repoStore.refreshRepo(); }
      : async () => await handleDiscardChanges(path);
  } finally {
    uiStore.setLoading(false);
  }
};

const handleIgnoreFile = async (file: FileStatus) => {
  let mutated = false;
  try {
    await withRepoLock('ignore', repoStore.repoInfo?.path, async () => {
      await gitService.addToGitignore(file.path);
    });
    mutated = true;
    await repoStore.refreshRepo();
    toast.success(`Added ${file.path} to .gitignore`, { title: 'Success' });
  } catch (e) {
    uiStore.setError(String(e));
    uiStore.lastFailedOperation = mutated
      ? async () => { await repoStore.refreshRepo(); }
      : async () => await handleIgnoreFile(file);
  }
};

const statusLetter = (file: FileStatus): string => {
  const first = file.status.charAt(0);
  return first ? first.toUpperCase() : '?';
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
          toast.error('Failed to copy path', { title: 'Clipboard Error' });
          console.error('Clipboard error:', err);
        }
      }
    },
    {
      label: 'Ignore File',
      action: () => handleIgnoreFile(file)
    },
    {
      label: 'Copy File Contents',
      action: async () => {
        try {
          const content = await gitService.readFile(file.path);
          await navigator.clipboard.writeText(content);
          toast.success('File contents copied', { title: 'Copied' });
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
            const abs = await gitService.resolveRepoFile(file.path);
            await gitService.revealInFinder(abs);
          } catch (e) {
            uiStore.setError(String(e));
          }
        }
      }
    },
    {
      label: 'Open in Editor',
      action: async () => {
        try {
          const abs = await gitService.resolveRepoFile(file.path);
          await openPath(abs);
        } catch (e) {
          uiStore.setError(String(e));
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
      action: () => emit('toggleAllStaged')
    },
    { divider: true },
    {
      label: 'Discard All Changes',
      danger: true,
      action: () => emit('handleDiscardAllChanges')
    }
  ]);
};
</script>

<template>
  <div class="flex flex-col flex-1 min-h-0">
    <!-- Changes Header with Bulk Select -->
    <div v-if="repoStore.fileStatuses.length > 0" 
         @contextmenu.prevent="onFileHeaderContextMenu"
         class="flex items-center gap-3 p-2.5 mb-2 rounded-lg bg-muted/50 border border-border transition-safe justify-between flex-shrink-0">
      <div class="flex items-center gap-3 cursor-pointer" @click="emit('toggleAllStaged')">
        <input type="checkbox" :checked="repoStore.allStaged" class="w-4 h-4 rounded border-border accent-accent cursor-pointer pointer-events-none" />
        <div class="text-xs font-semibold text-muted-foreground select-none">
          {{ repoStore.fileStatuses.length }} changed file{{ repoStore.fileStatuses.length !== 1 ? 's' : '' }}
        </div>
      </div>
      <button @click.stop="emit('handleDiscardAllChanges')" class="text-[10px] text-error hover:underline font-bold px-2 py-1 rounded hover:bg-error/10 transition-safe">DISCARD ALL</button>
    </div>

    <DynamicScroller
      v-if="repoStore.fileStatuses.length > 0"
      :items="repoStore.fileStatuses"
      :min-item-size="44"
      key-field="path"
      class="flex-1 overflow-auto"
    >
      <template #default="{ item, index, active }">
        <DynamicScrollerItem :item="item" :active="active" :data-index="index">
          <div class="pb-1.5">
            <div @contextmenu.prevent="onFileContextMenu($event, item)"
                 class="group flex items-center gap-2.5 px-2.5 py-2 rounded-lg cursor-pointer transition-safe"
                 :style="repoStore.selectedFile === item.path ? 'background:var(--spotlight); border:1px solid var(--accent); border-opacity:0.3;' : 'border:1px solid transparent;'"
                 :class="{ 'hover:bg-muted': repoStore.selectedFile !== item.path }"
                 @click.self="repoStore.selectedFile = item.path">
              <input type="checkbox" :checked="item.staged" @change="toggleStaged(item)" class="w-3.5 h-3.5 rounded flex-shrink-0" style="accent-color: var(--accent);" />
              <div class="flex-1 min-w-0 flex items-center gap-2" @click="repoStore.selectedFile = item.path">
                <span class="text-[10px] w-4 text-center font-bold flex-shrink-0"
                  :style="item.status === 'added' ? 'color:var(--success)' : item.status === 'deleted' ? 'color:var(--error)' : 'color:var(--accent)'">
                  {{ statusLetter(item) }}
                </span>
                <span class="truncate text-[13px]" :title="item.path">{{ item.path.split('/').pop() }}</span>
                <span class="text-[10px] font-mono truncate flex-shrink-0" style="color:var(--muted-foreground); max-width:60px;">{{ item.path.includes('/') ? item.path.substring(0, item.path.lastIndexOf('/')) : '' }}</span>
              </div>
              <button @click.stop="handleDiscardChanges(item.path)" class="opacity-0 group-hover:opacity-100 w-5 h-5 flex items-center justify-center rounded transition-safe flex-shrink-0 text-[10px] hover:bg-[var(--error-bg)]" style="color:var(--error);">✕</button>
            </div>
          </div>
        </DynamicScrollerItem>
      </template>
    </DynamicScroller>
  </div>
</template>
