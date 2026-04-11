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
}

const toasts = ref<Toast[]>([]);
const MAX_TOASTS = 5;

export function useToast() {
  const addToast = (type: ToastType, message: string, options?: ToastOptions) => {
    const id = Math.random().toString(36).substring(2, 9);
    const timeout = options?.timeout ?? 4000;
    
    const toast: Toast = {
      id,
      type,
      message,
      title: options?.title,
      timeout,
    };

    toasts.value.push(toast);

    if (toasts.value.length > MAX_TOASTS) {
      toasts.value.shift();
    }

    if (timeout > 0) {
      setTimeout(() => {
        dismiss(id);
      }, timeout);
    }

    return id;
  };

  const dismiss = (id: string) => {
    const index = toasts.value.findIndex((t) => t.id === id);
    if (index !== -1) {
      toasts.value.splice(index, 1);
    }
  };

  const success = (message: string, options?: ToastOptions) => addToast('success', message, options);
  const error = (message: string, options?: ToastOptions) => addToast('error', message, options);
  const warning = (message: string, options?: ToastOptions) => addToast('warning', message, options);
  const info = (message: string, options?: ToastOptions) => addToast('info', message, options);

  return {
    toasts,
    success,
    error,
    warning,
    info,
    dismiss,
  };
}
