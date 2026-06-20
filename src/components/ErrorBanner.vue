<script setup lang="ts">
import { useUIStore } from '../stores/ui';

const uiStore = useUIStore();

const retryLastOperation = async () => {
  if (uiStore.lastFailedOperation) {
    await uiStore.lastFailedOperation();
  }
};
</script>

<template>
  <div v-if="uiStore.error" class="border-b px-4 py-2.5 text-[13px] flex justify-between items-center" style="background: var(--error-bg); border-color: rgba(248,81,73,0.2); color: var(--error);">
    <span class="font-medium truncate mr-3">{{ uiStore.error }}</span>
    <div class="flex gap-2">
      <button v-if="uiStore.lastFailedOperation" @click="retryLastOperation" class="btn btn-danger text-xs px-2.5 py-1">Retry</button>
      <button @click="uiStore.clearError" class="btn btn-ghost text-xs px-2 py-1 ml-1">✕</button>
    </div>
  </div>
</template>
