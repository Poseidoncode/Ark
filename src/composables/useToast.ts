import { ref } from 'vue';

export type ToastType = 'success' | 'error' | 'warning' | 'info';

export interface ToastOptions {
  timeout?: number;
  title?: string;
}

export interface Toast {
  id: string;
  type: ToastType;
  message: string;
  title?: string;
  timeout: number;
  createdAt: number;
}

// Use module-level state for singleton behavior
const toasts = ref<Toast[]>([]);
const MAX_TOASTS = 5;
const timeoutIds = ref<Map<string, ReturnType<typeof setTimeout>>>(new Map());

export function useToast() {
  const addToast = (type: ToastType, message: string, options?: ToastOptions): string => {
    const id = Math.random().toString(36).substring(2, 9);
    const timeout = options?.timeout ?? 4000;

    const toast: Toast = {
      id,
      type,
      message,
      title: options?.title,
      timeout,
      createdAt: Date.now(),
    };

    // Add to beginning for newer toasts first
    toasts.value.unshift(toast);

    // Enforce max toasts limit
    while (toasts.value.length > MAX_TOASTS) {
      const oldest = toasts.value.pop();
      if (oldest) {
        const oldTimeout = timeoutIds.value.get(oldest.id);
        if (oldTimeout) {
          clearTimeout(oldTimeout);
          timeoutIds.value.delete(oldest.id);
        }
      }
    }

    // Set auto-dismiss timeout
    if (timeout > 0) {
      const timeoutId = setTimeout(() => {
        dismiss(id);
      }, timeout);
      timeoutIds.value.set(id, timeoutId);
    }

    return id;
  };

  const dismiss = (id: string) => {
    const index = toasts.value.findIndex((t) => t.id === id);
    if (index !== -1) {
      toasts.value.splice(index, 1);
    }

    // Clear timeout if exists
    const timeoutId = timeoutIds.value.get(id);
    if (timeoutId) {
      clearTimeout(timeoutId);
      timeoutIds.value.delete(id);
    }
  };

  const dismissAll = () => {
    // Clear all timeouts
    for (const timeoutId of timeoutIds.value.values()) {
      clearTimeout(timeoutId);
    }
    timeoutIds.value.clear();
    toasts.value = [];
  };

  const success = (message: string, options?: ToastOptions) => addToast('success', message, options);
  const error = (message: string, options?: ToastOptions) => addToast('error', message, options);
  const warning = (message: string, options?: ToastOptions) => addToast('warning', message, options);
  const info = (message: string, options?: ToastOptions) => addToast('info', message, options);

  return {
    toasts,
    addToast,
    dismiss,
    dismissAll,
    success,
    error,
    warning,
    info,
  };
}

