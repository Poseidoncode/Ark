<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { useRepoStore } from '../stores/repo';
import { useUIStore } from '../stores/ui';
import { useSettingsStore } from '../stores/settings';
import { gitService } from '../services/git';

const repoStore = useRepoStore();
const uiStore = useUIStore();
const settingsStore = useSettingsStore();

const dropdownRef = ref<HTMLElement | null>(null);

const currentProjectName = computed(() => {
  if (!repoStore.repoInfo) return "";
  
  const path = repoStore.repoInfo.path;
  const name = getRepoName(path);
  
  // 額外驗證：如果得到的名字和分支名相同，可能路徑有問題
  if (name === repoStore.repoInfo.current_branch) {
    console.warn('[WARNING] Project name equals branch name, path might be incorrect:', path);
    if (path && (path.includes('/') || path.includes('\\'))) {
      return getRepoName(path);
    }
    return path || "";
  }
  
  return name;
});

const getRepoName = (path: string) => {
  if (!path || path.trim() === "") return "";
  
  // 移除尾部的斜線
  const cleanPath = path.replace(/[/\\]+$/, '');
  
  // 如果路徑以 .git 結尾，取父目錄名
  if (cleanPath.endsWith('.git')) {
    const withoutGit = cleanPath.slice(0, -4).replace(/[/\\]+$/, '');
    const parts = withoutGit.split(/[/\\]/);
    return parts[parts.length - 1] || "";
  }
  
  // 取最後一個路徑段
  const parts = cleanPath.split(/[/\\]/);
  const lastPart = parts[parts.length - 1];
  
  // 如果最後一部分看起來不像是目錄名（太短或只是一個點），返回倒數第二個
  if (!lastPart || lastPart === '.' || lastPart === '..') {
    return parts[parts.length - 2] || path;
  }
  
  return lastPart || path;
};

const emit = defineEmits<{
  (e: 'openRepo', path?: string): void;
  (e: 'toggleRecentRepos'): void;
  (e: 'triggerCloneModal'): void;
  (e: 'handleFetch'): void;
  (e: 'loadTags'): void;
  (e: 'loadRemotes'): void;
  (e: 'openSettings'): void;
  (e: 'toggleTheme'): void;
  (e: 'openBranch'): void;
}>();

// Expose methods for parent
const handleOpenRepo = (path?: string) => {
  emit('openRepo', path);
};

onMounted(() => {
  window.addEventListener('click', handleClickOutside);
});

onUnmounted(() => {
  window.removeEventListener('click', handleClickOutside);
});

const handleClickOutside = (event: MouseEvent) => {
  if (uiStore.showRecentRepos && dropdownRef.value && !dropdownRef.value.contains(event.target as Node)) {
    uiStore.closeModal('recentRepos');
  }
};
</script>

<template>
  <header class="h-12 border-b border-border flex items-center px-4 justify-between flex-shrink-0 relative top-accent-border" style="background: var(--header-bg);">
    <div class="flex items-center gap-6 text-sm">
      <div ref="dropdownRef" class="relative">
        <div class="flex items-center gap-2 cursor-pointer px-2.5 py-1.5 rounded-lg transition-safe hover:bg-muted" :class="{ 'bg-muted': uiStore.showRecentRepos }" style="color: var(--foreground);" @click="uiStore.showRecentRepos = !uiStore.showRecentRepos">
          <!-- Ark Logo -->
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="color: var(--mark); flex-shrink:0;">
            <path d="M12 2L2 7l10 5 10-5-10-5z"/>
            <path d="M2 17l10 5 10-5"/>
            <path d="M2 12l10 5 10-5"/>
          </svg>
          <div class="flex items-center gap-1.5">
            <span class="text-[11px] font-medium" style="color: var(--muted-foreground);">repo</span>
            <span class="font-semibold text-[13px]" style="color: var(--foreground);">{{ repoStore.repoInfo ? currentProjectName : 'None' }}</span>
          </div>
          <div v-if="repoStore.repoInfo && (repoStore.repoInfo.ahead > 0 || repoStore.repoInfo.behind > 0)" class="flex items-center gap-1 ml-1">
            <span v-if="repoStore.repoInfo.ahead > 0" class="badge text-[10px] font-bold" style="background: var(--success-bg); color: var(--success);">↑{{ repoStore.repoInfo.ahead }}</span>
            <span v-if="repoStore.repoInfo.behind > 0" class="badge text-[10px] font-bold" style="background: var(--error-bg); color: var(--error);">↓{{ repoStore.repoInfo.behind }}</span>
          </div>
          <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" class="text-muted-foreground transition-transform duration-200" :class="{ 'rotate-180': uiStore.showRecentRepos }">
            <polyline points="6 9 12 15 18 9"/>
          </svg>
        </div>

        <!-- Recent Repositories Dropdown -->
        <div v-if="uiStore.showRecentRepos"
             class="absolute top-full left-0 mt-2 w-80 rounded-xl z-50 overflow-hidden py-1.5 glass shadow-xl"
             :style="{ border: '1px solid var(--border)', animation: 'slide-down 0.2s cubic-bezier(0.16,1,0.3,1)' }">
          <div class="px-4 py-2 border-b flex justify-between items-center" style="border-color: var(--border);">
            <span class="text-[10px] font-bold uppercase tracking-widest" style="color: var(--muted-foreground);">Recent Repositories</span>
            <button @click="handleOpenRepo()" class="text-[10px] font-bold transition-safe hover:underline" style="color: var(--accent);">OPEN NEW</button>
          </div>
          <div class="max-h-64 overflow-y-auto">
            <div v-for="path in settingsStore.settings?.recent_repositories" :key="path"
                 @click="handleOpenRepo(path)"
                 class="px-4 py-2.5 cursor-pointer transition-safe flex flex-col gap-0.5 hover:bg-muted"
                 :class="{ 'bg-spotlight': repoStore.repoInfo?.path === path }">
              <div class="text-[13px] font-semibold truncate flex items-center justify-between gap-2">
                <div class="flex items-center gap-2 truncate">
                  <span v-if="repoStore.repoInfo?.path === path" class="w-1.5 h-1.5 rounded-full flex-shrink-0" style="background: var(--accent);"></span>
                  {{ getRepoName(path) }}
                </div>
                <div v-if="repoStore.getRecentRepoInfo(path)" class="flex items-center gap-1 flex-shrink-0">
                  <span v-if="repoStore.getRecentRepoInfo(path)?.ahead" class="badge text-[9px] font-bold" style="background:var(--success-bg);color:var(--success);">↑{{ repoStore.getRecentRepoInfo(path)?.ahead }}</span>
                  <span v-if="repoStore.getRecentRepoInfo(path)?.behind" class="badge text-[9px] font-bold" style="background:var(--error-bg);color:var(--error);">↓{{ repoStore.getRecentRepoInfo(path)?.behind }}</span>
                  <span v-if="repoStore.getRecentRepoInfo(path)?.is_dirty" class="w-1.5 h-1.5 rounded-full" style="background:var(--warning);"></span>
                </div>
              </div>
              <div class="text-[10px] font-mono truncate" style="color: var(--muted-foreground);">{{ path }}</div>
            </div>
          </div>
          <div v-if="!settingsStore.settings?.recent_repositories.length" class="px-4 py-4 text-center text-xs italic" style="color: var(--muted-foreground);">No recent repositories</div>
        </div>
      </div>

      <div v-if="repoStore.repoInfo" class="flex items-center gap-2 cursor-pointer px-2.5 py-1.5 rounded-lg transition-safe hover:bg-muted" @click="uiStore.openModal('branch')" style="color: var(--foreground);">
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="color: var(--muted-foreground);">
          <line x1="6" y1="3" x2="6" y2="15"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/>
          <path d="M18 9a9 9 0 0 1-9 9"/>
        </svg>
        <span class="font-medium text-[13px]" style="color: var(--foreground);">{{ repoStore.getCurrentBranch() }}</span>
      </div>
    </div>

    <!-- Center title -->
    <div v-if="repoStore.repoInfo" class="absolute left-1/2 -translate-x-1/2 hidden md:flex items-center gap-2 pointer-events-none">
      <span class="text-[11px] font-semibold uppercase tracking-widest" style="color: var(--muted-foreground);">{{ currentProjectName }}</span>
    </div>

    <!-- Right Actions -->
    <div class="flex items-center gap-1.5">
      <button v-if="repoStore.repoInfo" @click="emit('triggerCloneModal')" class="btn btn-ghost h-8 px-3 text-[13px]">Clone</button>
      <button v-if="repoStore.repoInfo" @click="emit('handleFetch')" class="btn btn-ghost h-8 px-3 text-[13px]">Fetch</button>
      <button v-if="repoStore.repoInfo" @click="() => { emit('loadTags'); uiStore.openModal('tags'); }" class="btn btn-ghost h-8 px-3 text-[13px]">Tags</button>
      <button v-if="repoStore.repoInfo" @click="() => { emit('loadRemotes'); uiStore.openModal('remotes'); }" class="btn btn-ghost h-8 px-3 text-[13px]">Remotes</button>
      <div class="w-px h-5 mx-1" style="background: var(--border);"></div>
      <button @click="uiStore.openModal('settings')" class="btn btn-ghost h-8 w-8 p-0" title="Settings" style="color: var(--foreground);">
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>
        </svg>
      </button>
      <button @click="settingsStore.toggleTheme()" class="btn btn-ghost h-8 w-8 p-0" :title="settingsStore.settings?.theme === 'dark' ? 'Light Mode' : 'Dark Mode'" style="color: var(--foreground);">
        <svg v-if="settingsStore.settings?.theme === 'dark'" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="5"/><line x1="12" y1="1" x2="12" y2="3"/><line x1="12" y1="21" x2="12" y2="23"/><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/><line x1="1" y1="12" x2="3" y2="12"/><line x1="21" y1="12" x2="23" y2="12"/><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/>
        </svg>
        <svg v-else width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/>
        </svg>
      </button>
    </div>
  </header>
</template>
