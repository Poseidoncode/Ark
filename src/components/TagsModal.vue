<script setup lang="ts">
import { useUIStore } from '../stores/ui';
import { gitService } from '../services/git';
import { useToast } from '../composables/useToast';

const uiStore = useUIStore();
const toast = useToast();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'deleteTag', name: string): void;
}>();

const handleDeleteTag = async (name: string) => {
  const { ask } = await import('@tauri-apps/plugin-dialog');
  const confirmed = await ask(`Delete tag "${name}"?`, { title: 'Delete Tag', kind: 'warning' });
  if (!confirmed) return;
  try {
    uiStore.setLoading(true, "Deleting tag...");
    await gitService.deleteTag(name);
    // Reload tags
    const tagsList = await gitService.listTags();
    uiStore.setTags(tagsList);
    toast.success(`Tag "${name}" deleted`, { title: 'Success' });
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => emit('deleteTag', name);
  } finally {
    uiStore.setLoading(false);
  }
};
</script>

<template>
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
</template>
