<script setup lang="ts">
import { useUIStore } from '../stores/ui';
import { useRepoStore } from '../stores/repo';
import { useSettingsStore } from '../stores/settings';
import { gitService } from '../services/git';
import { homeDir } from '@tauri-apps/api/path';

const uiStore = useUIStore();
const repoStore = useRepoStore();
const settingsStore = useSettingsStore();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'browse'): void;
  (e: 'clone'): void;
}>();

// Watch for URL changes to auto-populate path
const handleUrlChange = async () => {
  if (uiStore.cloneUrl) {
    const match = uiStore.cloneUrl.match(/\/([^\/]+?)(\.git)?$/);
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
};

// Watch for URL changes
import { watch } from 'vue';
watch(() => uiStore.cloneUrl, handleUrlChange);
</script>

<template>
  <div class="bg-card rounded-2xl shadow-xl p-8 w-full max-w-md border border-border">
    <h2 class="text-2xl font-display mb-6 text-foreground">Clone Repository</h2>
    
    <div class="mb-5">
      <label class="block mb-2 text-sm font-medium text-foreground">Remote URL</label>
      <input v-model="uiStore.cloneUrl" placeholder="https://github.com/user/repo.git" class="w-full border border-border rounded-lg p-3 text-foreground text-sm focus:ring-2 focus:ring-accent focus:border-transparent outline-none" />
    </div>

    <div class="mb-8">
      <label class="block mb-2 text-sm font-medium text-foreground">Destination Path</label>
      <div class="flex gap-2">
        <input v-model="uiStore.clonePath" placeholder="/path/to/destination" class="flex-1 border border-border rounded-lg p-3 text-foreground text-sm focus:ring-2 focus:ring-accent focus:border-transparent outline-none" />
        <button @click="emit('browse')" class="px-4 py-3 border border-border rounded-lg hover:bg-muted transition-safe text-sm font-medium">Browse</button>
      </div>
    </div>

    <div class="flex justify-end gap-3">
      <button @click="uiStore.closeModal('clone')" class="px-6 py-2.5 border border-border rounded-lg hover:bg-muted transition-safe font-medium">Cancel</button>
      <button @click="emit('clone')" :disabled="!uiStore.cloneUrl || !uiStore.clonePath" class="gradient-bg text-accent-foreground px-6 py-2.5 rounded-lg disabled:opacity-50 disabled:cursor-not-allowed hover:shadow-accent transition-safe font-semibold">Clone</button>
    </div>
  </div>
</template>
