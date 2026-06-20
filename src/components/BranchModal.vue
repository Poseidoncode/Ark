<script setup lang="ts">
import { useUIStore } from '../stores/ui';
import { useRepoStore } from '../stores/repo';
import { gitService } from '../services/git';

const uiStore = useUIStore();
const repoStore = useRepoStore();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'checkout', branchName: string): void;
  (e: 'createBranch'): void;
}>();

const checkoutBranch = async (branchName: string) => {
  try {
    uiStore.setLoading(true, "Checking out branch...", false);
    uiStore.clearError();
    await gitService.checkoutBranch(branchName);
    uiStore.closeModal('branch');
    repoStore.clearSelection();
    await repoStore.refreshRepo();
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => emit('checkout', branchName);
  } finally {
    uiStore.setLoading(false);
  }
};

const handleCreateBranch = async () => {
  if (!uiStore.newBranchName.trim()) return;
  try {
    uiStore.setLoading(true, "Creating branch...", false);
    await gitService.createBranch(uiStore.newBranchName.trim());
    uiStore.setNewBranchName("");
    uiStore.closeModal('branch');
    await repoStore.refreshRepo();
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => emit('createBranch');
  } finally {
    uiStore.setLoading(false);
  }
};
</script>

<template>
  <div v-if="uiStore.showBranchModal" class="bg-card rounded-2xl shadow-xl p-8 w-full max-w-md border border-border">
    <h2 class="text-2xl font-display mb-6 text-foreground">Branches</h2>
    <div class="max-h-60 overflow-auto mb-6 space-y-2">
      <div v-for="branch in repoStore.branches" :key="branch.name"
           @click="!branch.is_current && checkoutBranch(branch.name)"
           class="p-3 rounded-lg border border-transparent hover:border-border cursor-pointer flex items-center justify-between text-sm transition-safe"
           :class="{ 'gradient-bg text-accent-foreground border-accent shadow-accent': branch.is_current, 'hover:bg-muted': !branch.is_current }">
        <span class="font-medium">{{ branch.name }}</span>
        <span v-if="branch.is_current" class="text-xs font-semibold">Active</span>
      </div>
    </div>
    <div class="border-t border-border pt-6">
      <label class="block text-sm font-medium text-foreground mb-2">Create New Branch</label>
      <div class="flex gap-2">
        <input v-model="uiStore.newBranchName" @keyup.enter="handleCreateBranch" placeholder="feature/new-branch" class="flex-1 border border-border rounded-lg p-3 text-foreground text-sm outline-none focus:ring-2 focus:ring-accent focus:border-transparent font-mono" />
        <button @click="handleCreateBranch" class="gradient-bg text-accent-foreground px-5 rounded-lg hover:shadow-accent transition-safe font-semibold">+</button>
      </div>
    </div>
    <div class="flex justify-end mt-6">
      <button @click="uiStore.closeModal('branch')" class="px-6 py-2.5 border border-border rounded-lg hover:bg-muted transition-safe font-medium">Close</button>
    </div>
  </div>
</template>
