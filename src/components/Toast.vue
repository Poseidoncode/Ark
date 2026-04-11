<template>
  <div class="fixed top-4 right-4 z-50 flex flex-col gap-2 pointer-events-none">
    <TransitionGroup 
      name="toast" 
      tag="div" 
      class="flex flex-col gap-2"
    >
      <div 
        v-for="toast in toasts" 
        :key="toast.id"
        class="pointer-events-auto flex items-start p-4 w-80 max-w-full bg-card border border-border shadow-lg rounded-xl overflow-hidden transition-all relative"
        role="alert"
      >
        <div class="absolute left-0 top-0 bottom-0 w-1" :class="typeClasses[toast.type].borderClass"></div>

        <div class="flex-shrink-0 mr-3 mt-0.5" :class="typeClasses[toast.type].iconClass">
          <svg v-if="toast.type === 'success'" xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"></path>
            <polyline points="22 4 12 14.01 9 11.01"></polyline>
          </svg>
          
          <svg v-else-if="toast.type === 'error'" xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="10"></circle>
            <line x1="15" y1="9" x2="9" y2="15"></line>
            <line x1="9" y1="9" x2="15" y2="15"></line>
          </svg>

          <svg v-else-if="toast.type === 'warning'" xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z"></path>
            <line x1="12" y1="9" x2="12" y2="13"></line>
            <line x1="12" y1="17" x2="12.01" y2="17"></line>
          </svg>

          <svg v-else xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="10"></circle>
            <line x1="12" y1="16" x2="12" y2="12"></line>
            <line x1="12" y1="8" x2="12.01" y2="8"></line>
          </svg>
        </div>

        <div class="flex-1 min-w-0 mr-2">
          <p v-if="toast.title" class="text-sm font-semibold text-foreground mb-1">{{ toast.title }}</p>
          <p class="text-sm text-muted-foreground break-words">{{ toast.message }}</p>
        </div>

        <button 
          @click="dismiss(toast.id)"
          class="flex-shrink-0 text-muted-foreground hover:text-foreground transition-colors p-1 -mr-2 -mt-2 rounded-full hover:bg-muted"
          aria-label="Close"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      </div>
    </TransitionGroup>
  </div>
</template>

<script setup lang="ts">
import { useToast } from '../composables/useToast';

const { toasts, dismiss } = useToast();

const typeClasses = {
  success: {
    borderClass: 'bg-success',
    iconClass: 'text-success'
  },
  error: {
    borderClass: 'bg-error',
    iconClass: 'text-error'
  },
  warning: {
    borderClass: 'bg-warning',
    iconClass: 'text-warning'
  },
  info: {
    borderClass: 'bg-accent',
    iconClass: 'text-accent'
  }
};
</script>

<style scoped>
.toast-enter-active,
.toast-leave-active {
  transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}
.toast-enter-from {
  opacity: 0;
  transform: translateX(100%) scale(0.9);
}
.toast-leave-to {
  opacity: 0;
  transform: scale(0.9) translateY(-10px);
}
.toast-leave-active {
  position: absolute;
}
.toast-move {
  transition: transform 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}
</style>
