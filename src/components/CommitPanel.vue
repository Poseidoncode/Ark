<script setup lang="ts">
import { useRepoStore } from '../stores/repo';
import { useUIStore } from '../stores/ui';
import { gitService } from '../services/git';
import { useToast } from '../composables/useToast';

const repoStore = useRepoStore();
const uiStore = useUIStore();
const toast = useToast();

const emit = defineEmits<{
  (e: 'commit'): void;
}>();

const handleCommit = async () => {
  if (!uiStore.commitMessage.trim()) {
    toast.error("Please enter a commit message", { title: "Commit Error" });
    return;
  }
  if (!uiStore.amendCommit && repoStore.stagedFiles.length === 0) {
    toast.error("Please select files to commit", { title: "Commit Error" });
    return;
  }

  try {
    uiStore.setLoading(true, "Creating commit...", true);
    uiStore.clearError();
    if (uiStore.amendCommit) {
      await gitService.amendCommit(uiStore.commitMessage);
      toast.success("Commit amended successfully!", { title: "Success" });
      uiStore.setAmendCommit(false);
    } else {
      await gitService.createCommit(uiStore.commitMessage, repoStore.stagedFiles);
      toast.success("Commit created successfully!", { title: "Success" });
    }
    uiStore.setCommitMessage("");
    repoStore.selectedFile = null;
    await repoStore.refreshRepo();
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => emit('commit');
  } finally {
    uiStore.setLoading(false);
  }
};
</script>

<template>
  <div class="p-4 border-t border-border bg-muted/30">
    <div class="flex items-center gap-2 mb-2">
       <input type="checkbox" id="amend" v-model="uiStore.amendCommit" class="w-3.5 h-3.5 rounded border-border accent-accent" />
       <label for="amend" class="text-xs font-medium text-muted-foreground cursor-pointer select-none">Amend Last Commit</label>
    </div>
    <label class="block mb-2 text-sm font-medium text-foreground">Commit Message</label>
    <textarea v-model="uiStore.commitMessage" placeholder="Describe your changes..." class="w-full bg-card border border-border rounded-lg p-3 text-foreground text-sm mb-3 focus:ring-2 focus:ring-accent focus:border-transparent outline-none resize-none" rows="3" />
    <button @click="handleCommit" :disabled="uiStore.loading || !uiStore.commitMessage.trim() || (!uiStore.amendCommit && repoStore.stagedFiles.length === 0)" 
            class="w-full gradient-bg text-accent-foreground disabled:opacity-50 disabled:cursor-not-allowed py-2.5 rounded-lg font-semibold text-sm hover:shadow-accent transition-safe">
      {{ uiStore.amendCommit ? 'Amend Commit' : `Commit to ${repoStore.getCurrentBranch()}` }}
    </button>
  </div>
</template>
