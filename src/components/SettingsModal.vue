<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { useUIStore } from '../stores/ui';
import { useSettingsStore } from '../stores/settings';
import { useRepoStore } from '../stores/repo';
import { gitService, type GitHubUser } from '../services/git';
import { useToast } from '../composables/useToast';

const uiStore = useUIStore();
const settingsStore = useSettingsStore();
const repoStore = useRepoStore();
const toast = useToast();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'save'): void;
  (e: 'switchToSSH'): void;
}>();

// ── GitHub OAuth status ───────────────────────────────────────
const githubUser = ref<GitHubUser | null>(null);
const checkingAuth = ref(true);

const checkAuthStatus = async () => {
  checkingAuth.value = true;
  try {
    githubUser.value = await gitService.oauthGetStatus();
  } catch {
    githubUser.value = null;
  } finally {
    checkingAuth.value = false;
  }
};

onMounted(() => {
  checkAuthStatus();
});

const handleOpenAuth = () => {
  uiStore.closeModal('settings');
  uiStore.openModal('auth');
};

const handleSignOut = async () => {
  try {
    await gitService.oauthSignOut();
    githubUser.value = null;
    toast.success('Signed out from GitHub', { title: 'Disconnected' });
  } catch (err) {
    toast.error('Failed to sign out: ' + String(err), { title: 'Error' });
  }
};

const handleSwitchToSSH = async () => {
  if (!repoStore.repoInfo) return;
  try {
    uiStore.setLoading(true, "Switching remote to SSH...", false);
    uiStore.clearError();
    
    const currentUrl = await gitService.getRemoteUrl("origin");
    
    if (!currentUrl) {
      toast.error("No remote 'origin' found", { title: "Error" });
      return;
    }
    
    let ownerRepo = "";
    let sshUrl = "";
    
    if (currentUrl.startsWith("https://")) {
      const match = currentUrl.match(/github\.com\/([^\/]+\/[^\/]+)/);
      if (match) {
        ownerRepo = match[1].replace(/\.git$/, '');
      }
      sshUrl = `git@github.com:${ownerRepo}.git`;
    } else if (currentUrl.startsWith("git@")) {
      toast.info("Remote is already using SSH protocol", { title: "Info" });
      uiStore.setLoading(false);
      return;
    } else {
      toast.error("Unsupported remote URL format", { title: "Error" });
      uiStore.setLoading(false);
      return;
    }
    
    if (!ownerRepo) {
      toast.error("Could not parse repository from remote URL", { title: "Error" });
      uiStore.setLoading(false);
      return;
    }
    
    const { ask } = await import('@tauri-apps/plugin-dialog');
    const confirmed = await ask(`Switch remote protocol to SSH?\nNew URL: ${sshUrl}`, { title: 'Switch Remote', kind: 'warning' });
    if (confirmed) {
      await gitService.setRemoteUrl("origin", sshUrl);
      toast.success("Remote protocol switched to SSH successfully!", { title: "Success" });
      uiStore.closeModal('settings');
    }
    uiStore.clearError();
  } catch (err) {
    uiStore.setError(String(err));
    uiStore.lastFailedOperation = async () => emit('switchToSSH');
  } finally {
    uiStore.setLoading(false);
  }
};

const saveSettings = async () => {
  if (settingsStore.settings) {
    await settingsStore.saveSettings();
    uiStore.closeModal('settings');
  }
};
</script>

<template>
  <div v-if="uiStore.showSettingsModal && settingsStore.settings" class="bg-card rounded-2xl shadow-xl p-8 w-full max-w-md border border-border">
    <h2 class="text-2xl font-display mb-6 text-foreground">Settings</h2>

    <!-- GitHub Authentication Section -->
    <div class="mb-6 p-4 rounded-lg border border-border">
      <label class="block text-sm font-semibold text-foreground mb-2">GitHub Authentication</label>
      <div v-if="checkingAuth" class="flex items-center gap-2 text-[12px] text-muted-foreground">
        <div class="w-3 h-3 border border-accent border-t-transparent rounded-full animate-spin"></div>
        <span>Checking status...</span>
      </div>
      <div v-else-if="githubUser" class="flex items-center justify-between">
        <div class="flex items-center gap-2">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor" style="color: var(--success);">
            <path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12"/>
          </svg>
          <span class="text-sm font-medium text-foreground">{{ githubUser.login }}</span>
        </div>
        <button @click="handleSignOut" class="text-[11px] text-error hover:underline font-medium">Sign Out</button>
      </div>
      <div v-else>
        <p class="text-[11px] text-muted-foreground mb-2 leading-tight">Sign in to enable push, pull, and clone over HTTPS</p>
        <button @click="handleOpenAuth" class="text-sm text-accent hover:underline font-semibold flex items-center gap-2">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12"/></svg>
          Sign In with GitHub
        </button>
      </div>
    </div>

    <div class="space-y-5 mb-8">
      <div>
        <label class="block text-sm font-semibold text-foreground mb-1">Git User Name</label>
        <p class="text-[11px] text-muted-foreground mb-2 leading-tight">Identifies you as the author of commits</p>
        <input v-model="settingsStore.settings.user_name" class="input" />
      </div>
      <div>
        <label class="block text-sm font-semibold text-foreground mb-1">Git User Email</label>
        <p class="text-[11px] text-muted-foreground mb-2 leading-tight">Email address associated with your commits</p>
        <input v-model="settingsStore.settings.user_email" class="input" />
      </div>
      <div>
        <label class="block text-sm font-semibold text-foreground mb-1">SSH Key Path</label>
        <input v-model="settingsStore.settings.ssh_key_path" placeholder="~/.ssh/id_rsa" class="input font-mono" />
      </div>
      <div class="pt-4 border-t border-border">
        <button @click="handleSwitchToSSH" class="text-sm text-accent hover:underline font-semibold flex items-center gap-2">
          <span>⚠️</span> Switch remotes to SSH
        </button>
        <p class="text-[11px] text-muted-foreground mt-1 leading-tight">Use this if you get authentication errors with HTTPS</p>
      </div>
    </div>
    <div class="flex justify-end gap-3">
      <button @click="uiStore.closeModal('settings')" class="px-6 py-2.5 border border-border rounded-lg hover:bg-muted transition-safe font-medium">Cancel</button>
      <button @click="saveSettings" class="gradient-bg text-accent-foreground px-6 py-2.5 rounded-lg hover:shadow-accent transition-safe font-semibold">Save</button>
    </div>
  </div>
</template>
