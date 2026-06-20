<script setup lang="ts">
import { useUIStore } from '../stores/ui';

const uiStore = useUIStore();
</script>

<template>
  <!-- Loading Overlay (major operations) -->
  <div v-if="uiStore.loading && uiStore.isMajorOperation" class="fixed inset-0 z-[100] flex items-center justify-center" style="background: rgba(0,0,0,0.6); backdrop-filter: blur(8px);">
    <div class="rounded-2xl p-8 flex flex-col items-center gap-5" style="background:var(--card); border:1px solid var(--border); box-shadow:var(--shadow-xl);">
      <div class="spinner spinner-lg"></div>
      <span class="text-[15px] font-semibold">{{ uiStore.loadingMessage || 'Processing...' }}</span>
    </div>
  </div>
  <!-- Small loading indicator -->
  <div v-else-if="uiStore.loading" class="fixed bottom-4 right-4 z-[100] flex items-center gap-2 px-3 py-2 rounded-xl" style="background:var(--card); border:1px solid var(--border); box-shadow:var(--shadow-lg); backdrop-filter:blur(12px);">
    <div class="spinner"></div>
    <span class="text-[12px] font-medium" style="color:var(--muted-foreground);">{{ uiStore.loadingMessage || 'Loading...' }}</span>
  </div>
</template>
