<script setup lang="ts">
import { useUIStore } from '../stores/ui';
import { useSettingsStore } from '../stores/settings';
import { useRepoStore } from '../stores/repo';
import { gitService } from '../services/git';
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
    <div class="space-y-5 mb-8">
      <div>
        <label class="block text-sm font-semibold text-foreground mb-1">Git User Name</label>
        <p class="text-[11px] text-muted-foreground mb-2 leading-tight">Identifies you as the author of commits</p>
        <input v-model="settingsStore.settings.user_name" class="w-full border border-border rounded-lg p-3 text-foreground text-sm outline-none focus:ring-2 focus:ring-accent focus:border-transparent bg-white shadow-sm" />
      </div>
      <div>
        <label class="block text-sm font-semibold text-foreground mb-1">Git User Email</label>
        <p class="text-[11px] text-muted-foreground mb-2 leading-tight">Email address associated with your commits</p>
        <input v-model="settingsStore.settings.user_email" class="w-full border border-border rounded-lg p-3 text-foreground text-sm outline-none focus:ring-2 focus:ring-accent focus:border-transparent bg-white shadow-sm" />
      </div>
      <div>
        <label class="block text-sm font-semibold text-foreground mb-1">SSH Key Path</label>
        <input v-model="settingsStore.settings.ssh_key_path" placeholder="~/.ssh/id_rsa" class="w-full border border-border rounded-lg p-3 text-foreground text-sm outline-none focus:ring-2 focus:ring-accent focus:border-transparent font-mono bg-white shadow-sm" />
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
