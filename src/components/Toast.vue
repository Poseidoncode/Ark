<template>
  <div class="fixed top-4 right-4 z-[200] flex flex-col gap-2 pointer-events-none" style="max-width: 360px; width: 100%;">
    <TransitionGroup name="toast" tag="div" class="flex flex-col gap-2">
      <div
        v-for="toast in toasts"
        :key="toast.id"
        class="pointer-events-auto relative overflow-hidden rounded-xl border shadow-lg"
        :class="typeClasses[toast.type].wrapClass"
        role="alert"
        aria-live="polite"
      >
        <!-- Top accent line -->
        <div class="absolute top-0 left-0 right-0 h-[2px]" :class="typeClasses[toast.type].accentClass"></div>

        <div class="flex items-start gap-3 px-4 py-3.5">
          <!-- Icon -->
          <div class="flex-shrink-0 mt-0.5" :class="typeClasses[toast.type].iconClass">
            <!-- Success -->
            <svg v-if="toast.type === 'success'" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/>
              <polyline points="22 4 12 14.01 9 11.01"/>
            </svg>
            <!-- Error -->
            <svg v-else-if="toast.type === 'error'" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="12" cy="12" r="10"/>
              <line x1="15" y1="9" x2="9" y2="15"/>
              <line x1="9" y1="9" x2="15" y2="15"/>
            </svg>
            <!-- Warning -->
            <svg v-else-if="toast.type === 'warning'" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z"/>
              <line x1="12" y1="9" x2="12" y2="13"/>
              <line x1="12" y1="17" x2="12.01" y2="17"/>
            </svg>
            <!-- Info -->
            <svg v-else width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="12" cy="12" r="10"/>
              <line x1="12" y1="16" x2="12" y2="12"/>
              <line x1="12" y1="8" x2="12.01" y2="8"/>
            </svg>
          </div>

          <!-- Content -->
          <div class="flex-1 min-w-0">
            <p v-if="toast.title" class="text-sm font-semibold mb-0.5" style="color: var(--foreground);">{{ toast.title }}</p>
            <p class="text-xs leading-relaxed break-words" style="color: var(--muted-foreground);">{{ toast.message }}</p>
          </div>

          <!-- Dismiss -->
          <button
            @click="dismiss(toast.id)"
            class="flex-shrink-0 flex items-center justify-center w-6 h-6 rounded-md transition-safe -mt-0.5 -mr-1"
            style="color: var(--muted-foreground);"
            :style="{ ':hover': { background: 'var(--muted)', color: 'var(--foreground)' } }"
            aria-label="Dismiss"
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <line x1="18" y1="6" x2="6" y2="18"/>
              <line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>
    </TransitionGroup>
  </div>
</template>

<script setup lang="ts">
import { useToast } from '../composables/useToast';

const { toasts, dismiss } = useToast();

const typeClasses = {
  success: {
    wrapClass: 'toast-success',
    accentClass: 'bg-gradient-to-r from-green-500 to-emerald-400',
    iconClass: 'text-green-500',
  },
  error: {
    wrapClass: 'toast-error',
    accentClass: 'bg-gradient-to-r from-red-500 to-rose-400',
    iconClass: 'text-red-500',
  },
  warning: {
    wrapClass: 'toast-warning',
    accentClass: 'bg-gradient-to-r from-yellow-500 to-amber-400',
    iconClass: 'text-yellow-500',
  },
  info: {
    wrapClass: 'toast-info',
    accentClass: 'bg-gradient-to-r from-blue-500 to-indigo-400',
    iconClass: 'text-blue-500',
  },
};
</script>

<style scoped>
/* Toast base */
.toast-success,
.toast-error,
.toast-warning,
.toast-info {
  background: var(--card);
  border-color: var(--border);
  backdrop-filter: blur(16px);
  -webkit-backdrop-filter: blur(16px);
}

/* Toast transitions */
.toast-enter-active {
  transition: all 0.35s cubic-bezier(0.16, 1, 0.3, 1);
}
.toast-leave-active {
  transition: all 0.25s cubic-bezier(0.4, 0, 1, 1);
  position: absolute;
  right: 0;
  left: 0;
}
.toast-enter-from {
  opacity: 0;
  transform: translateX(24px) scale(0.95);
}
.toast-leave-to {
  opacity: 0;
  transform: translateX(24px) scale(0.95);
}
.toast-move {
  transition: transform 0.3s cubic-bezier(0.16, 1, 0.3, 1);
}

/* Dismiss button hover */
button:hover {
  background: var(--muted);
  color: var(--foreground);
}
</style>
