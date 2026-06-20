<script setup lang="ts">
import { useUIStore } from '../stores/ui';
import { gitService } from '../services/git';
import { useToast } from '../composables/useToast';

const uiStore = useUIStore();
const toast = useToast();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'addRemote'): void;
  (e: 'removeRemote', name: string): void;
}>();

const handleAddRemote = async () => {
  if (!uiStore.newRemoteName.trim() || !uiStore.newRemoteUrl.trim()) return;
  const name = uiStore.newRemoteName.trim();
  const url = uiStore.newRemoteUrl.trim();
  try {
    uiStore.setLoading(true, "Adding remote...");
    await gitService.addRemote(name, url);
    uiStore.clearNewRemoteFields();
    // Reload remotes
    const remotesList = await gitService.listRemotes();
    uiStore.setRemotes(remotesList);
    toast.success(`Remote "${name}" added`, { title: 'Success' });
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
  } finally {
    uiStore.setLoading(false);
  }
};

const handleRemoveRemote = async (name: string) => {
  const { ask } = await import('@tauri-apps/plugin-dialog');
  const confirmed = await ask(`Remove remote "${name}"?`, { title: 'Remove Remote', kind: 'warning' });
  if (!confirmed) return;
  try {
    uiStore.setLoading(true, "Removing remote...");
    await gitService.removeRemote(name);
    // Reload remotes
    const remotesList = await gitService.listRemotes();
    uiStore.setRemotes(remotesList);
    toast.success(`Remote "${name}" removed`, { title: 'Success' });
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => emit('removeRemote', name);
  } finally {
    uiStore.setLoading(false);
  }
};
</script>

<template>
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
</template>
