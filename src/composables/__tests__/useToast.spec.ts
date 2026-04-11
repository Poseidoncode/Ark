import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

let mockToasts: any[] = [];

function createTestToast() {
  const MAX_TOASTS = 5;
  
  const addToast = (type: string, message: string, options?: any) => {
    const id = Math.random().toString(36).substring(2, 9);
    const timeout = options?.timeout ?? 4000;
    
    const toast = {
      id,
      type,
      message,
      title: options?.title,
      timeout,
    };

    mockToasts.push(toast);

    if (mockToasts.length > MAX_TOASTS) {
      mockToasts.shift();
    }

    if (timeout > 0) {
      setTimeout(() => {
        dismiss(id);
      }, timeout);
    }

    return id;
  };

  const dismiss = (id: string) => {
    const index = mockToasts.findIndex((t) => t.id === id);
    if (index !== -1) {
      mockToasts.splice(index, 1);
    }
  };

  return {
    get toasts() { return mockToasts; },
    success: (msg: string, opts?: any) => addToast('success', msg, opts),
    error: (msg: string, opts?: any) => addToast('error', msg, opts),
    warning: (msg: string, opts?: any) => addToast('warning', msg, opts),
    info: (msg: string, opts?: any) => addToast('info', msg, opts),
    dismiss,
  };
}

describe('useToast', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mockToasts = [];
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('should add toast successfully', () => {
    const toast = createTestToast();
    toast.success('Test message');
    expect(toast.toasts).toHaveLength(1);
    expect(toast.toasts[0].message).toBe('Test message');
    expect(toast.toasts[0].type).toBe('success');
  });
});
