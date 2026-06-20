<script setup lang="ts">
import { useRepoStore } from '../stores/repo';
import { useUIStore } from '../stores/ui';
import { gitService, type FileStatus, type StageResult } from '../services/git';
import { useToast } from '../composables/useToast';
import { useContextMenu } from '../composables/useContextMenu';
import { openPath } from '@tauri-apps/plugin-opener';

const repoStore = useRepoStore();
const uiStore = useUIStore();
const toast = useToast();
const { showContextMenu, hideContextMenu } = useContextMenu();

const emit = defineEmits<{
  (e: 'toggleAllStaged'): void;
  (e: 'handleDiscardAllChanges'): void;
  (e: 'contextMenu', event: MouseEvent, file: FileStatus): void;
  (e: 'fileHeaderContextMenu', event: MouseEvent): void;
}>();

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
  const { ask } = await import('@tauri-apps/plugin-dialog');
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
  <div class="space-y-1.5">
    <!-- Changes Header with Bulk Select -->
    <div v-if="repoStore.fileStatuses.length > 0" 
         @contextmenu.prevent="onFileHeaderContextMenu"
         class="flex items-center gap-3 p-2.5 mb-2 rounded-lg bg-muted/50 border border-border transition-safe justify-between">
      <div class="flex items-center gap-3 cursor-pointer" @click="emit('toggleAllStaged')">
        <input type="checkbox" :checked="repoStore.allStaged" class="w-4 h-4 rounded border-border accent-accent cursor-pointer pointer-events-none" />
        <div class="text-xs font-semibold text-muted-foreground select-none">
          {{ repoStore.fileStatuses.length }} changed file{{ repoStore.fileStatuses.length !== 1 ? 's' : '' }}
        </div>
      </div>
      <button @click.stop="emit('handleDiscardAllChanges')" class="text-[10px] text-error hover:underline font-bold px-2 py-1 rounded hover:bg-error/10 transition-safe">DISCARD ALL</button>
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
</template>
