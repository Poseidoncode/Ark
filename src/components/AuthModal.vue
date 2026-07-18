<script setup lang="ts">
import { ref, onUnmounted } from 'vue';
import { gitService, type DeviceFlowInfo, type GitHubUser } from '../services/git';
import { useToast } from '../composables/useToast';

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'authenticated', user: GitHubUser): void;
}>();

const toast = useToast();

// ── State ──────────────────────────────────────────────────────
type FlowState = 'idle' | 'pending' | 'polling' | 'success' | 'error';

const flowState = ref<FlowState>('idle');
const deviceFlowInfo = ref<DeviceFlowInfo | null>(null);
const authUser = ref<GitHubUser | null>(null);
const errorMessage = ref('');
const isSigningOut = ref(false);

// Cancellation flag for polling
let cancelPolling = false;

// ── Actions ────────────────────────────────────────────────────

const handleSignIn = async () => {
  flowState.value = 'pending';
  errorMessage.value = '';
  cancelPolling = false;

  try {
    // Step 1: Request device code
    deviceFlowInfo.value = await gitService.oauthStartDeviceFlow();

    // Open the verification URI in the user's default browser
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    await openUrl(deviceFlowInfo.value.verification_uri);

    flowState.value = 'polling';

    // Step 2: Poll for the token
    const user = await gitService.oauthPollForToken(
      deviceFlowInfo.value.device_code,
      deviceFlowInfo.value.interval,
      deviceFlowInfo.value.expires_in,
    );

    if (cancelPolling) return;

    authUser.value = user;
    flowState.value = 'success';
    toast.success(`Signed in as ${user.login}`, { title: 'GitHub Connected' });
    emit('authenticated', user);
  } catch (err) {
    if (cancelPolling) return;
    errorMessage.value = String(err);
    flowState.value = 'error';
  }
};

const handleCancel = () => {
  cancelPolling = true;
  gitService.oauthCancelPolling();
  flowState.value = 'idle';
  deviceFlowInfo.value = null;
  errorMessage.value = '';
};

const handleSignOut = async () => {
  isSigningOut.value = true;
  try {
    await gitService.oauthSignOut();
    authUser.value = null;
    flowState.value = 'idle';
    deviceFlowInfo.value = null;
    toast.success('Signed out from GitHub', { title: 'Disconnected' });
  } catch (err) {
    toast.error('Failed to sign out: ' + String(err), { title: 'Error' });
  } finally {
    isSigningOut.value = false;
  }
};

const handleClose = () => {
  cancelPolling = true;
  gitService.oauthCancelPolling();
  emit('close');
};

onUnmounted(() => {
  cancelPolling = true;
  gitService.oauthCancelPolling();
});
</script>

<template>
  <div class="bg-card rounded-2xl shadow-xl p-8 w-full max-w-md border border-border">
    <h2 class="text-2xl font-display mb-2 text-foreground">GitHub Sign In</h2>
    <p class="text-[12px] text-muted-foreground mb-6 leading-relaxed">
      Authenticate with GitHub to enable push, pull, fetch, and clone over HTTPS.
      Your token is stored securely in the OS keychain.
    </p>

    <!-- Idle / Not Authenticated State -->
    <div v-if="flowState === 'idle'" class="space-y-4">
      <div class="flex items-center gap-3 p-4 rounded-lg border border-border bg-muted/30">
        <svg width="32" height="32" viewBox="0 0 24 24" fill="currentColor" style="color: var(--foreground);">
          <path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12"/>
        </svg>
        <div class="flex-1">
          <div class="text-sm font-semibold text-foreground">Connect to GitHub</div>
          <div class="text-[11px] text-muted-foreground">Uses OAuth Device Flow — no password needed</div>
        </div>
      </div>
      <button @click="handleSignIn" class="w-full gradient-bg text-accent-foreground px-6 py-3 rounded-lg hover:shadow-accent transition-safe font-semibold flex items-center justify-center gap-2">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12"/></svg>
        Sign In with GitHub
      </button>
    </div>

    <!-- Pending: Requesting device code -->
    <div v-else-if="flowState === 'pending'" class="flex flex-col items-center py-8">
      <div class="w-8 h-8 border-2 border-accent border-t-transparent rounded-full animate-spin mb-4"></div>
      <p class="text-sm text-muted-foreground">Contacting GitHub...</p>
    </div>

    <!-- Polling: Waiting for user to authorize -->
    <div v-else-if="flowState === 'polling' && deviceFlowInfo" class="space-y-5">
      <div class="text-center">
        <p class="text-sm text-foreground mb-1">Enter this code on GitHub:</p>
        <div class="text-3xl font-bold font-mono tracking-wider py-3 px-6 rounded-lg border-2 border-accent inline-block" style="background: var(--muted);">
          {{ deviceFlowInfo.user_code }}
        </div>
      </div>
      <div class="flex items-center justify-center gap-2 text-[12px] text-muted-foreground">
        <div class="w-3 h-3 border border-accent border-t-transparent rounded-full animate-spin"></div>
        <span>Waiting for authorization...</span>
      </div>
      <p class="text-[11px] text-muted-foreground text-center leading-relaxed">
        A browser should have opened to {{ deviceFlowInfo.verification_uri }}.<br>
        If it didn't, <a @click="import('@tauri-apps/plugin-opener').then(m => m.openUrl(deviceFlowInfo!.verification_uri))" class="text-accent hover:underline cursor-pointer">click here</a>.
      </p>
      <button @click="handleCancel" class="w-full px-6 py-2.5 border border-border rounded-lg hover:bg-muted transition-safe font-medium text-sm">
        Cancel
      </button>
    </div>

    <!-- Success: Authenticated -->
    <div v-else-if="flowState === 'success' && authUser" class="space-y-4">
      <div class="flex items-center gap-3 p-4 rounded-lg border border-border" style="background: var(--success-bg);">
        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="color: var(--success);">
          <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/>
        </svg>
        <div class="flex-1">
          <div class="text-sm font-semibold text-foreground">Connected as {{ authUser.login }}</div>
          <div class="text-[11px] text-muted-foreground">{{ authUser.name || authUser.email || 'GitHub account' }}</div>
        </div>
      </div>
      <p class="text-[12px] text-muted-foreground">
        You can now push, pull, fetch, and clone private repositories over HTTPS.
      </p>
      <div class="flex gap-3">
        <button @click="handleSignOut" :disabled="isSigningOut" class="flex-1 px-4 py-2.5 border border-border rounded-lg hover:bg-muted transition-safe font-medium text-sm disabled:opacity-50">
          Sign Out
        </button>
        <button @click="handleClose" class="flex-1 gradient-bg text-accent-foreground px-4 py-2.5 rounded-lg hover:shadow-accent transition-safe font-semibold text-sm">
          Done
        </button>
      </div>
    </div>

    <!-- Error State -->
    <div v-else-if="flowState === 'error'" class="space-y-4">
      <div class="flex items-start gap-3 p-4 rounded-lg border border-border" style="background: var(--error-bg);">
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="color: var(--error); flex-shrink: 0; margin-top: 2px;">
          <circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/>
        </svg>
        <div class="flex-1">
          <div class="text-sm font-semibold" style="color: var(--error);">Authentication Failed</div>
          <div class="text-[11px] mt-1 break-words" style="color: var(--error);">{{ errorMessage }}</div>
        </div>
      </div>
      <div class="flex gap-3">
        <button @click="handleClose" class="flex-1 px-4 py-2.5 border border-border rounded-lg hover:bg-muted transition-safe font-medium text-sm">Close</button>
        <button @click="handleSignIn" class="flex-1 gradient-bg text-accent-foreground px-4 py-2.5 rounded-lg hover:shadow-accent transition-safe font-semibold text-sm">Try Again</button>
      </div>
    </div>

    <!-- Close button (top-right) -->
    <button v-if="flowState === 'idle'" @click="handleClose" class="absolute top-4 right-4 text-muted-foreground hover:text-foreground transition-safe">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
      </svg>
    </button>
  </div>
</template>
