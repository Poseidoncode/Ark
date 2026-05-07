import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { useToast } from '../useToast';

describe('useToast', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    const { dismissAll } = useToast();
    dismissAll();
  });

  afterEach(() => {
    vi.useRealTimers();
    const { dismissAll } = useToast();
    dismissAll();
  });

  describe('addToast', () => {
    it('should add a toast with correct type and message', () => {
      const { success, toasts } = useToast();
      success('Test message');
      expect(toasts.value).toHaveLength(1);
      expect(toasts.value[0].message).toBe('Test message');
      expect(toasts.value[0].type).toBe('success');
    });

    it('should add error toast', () => {
      const { error, toasts } = useToast();
      error('Error message');
      expect(toasts.value).toHaveLength(1);
      expect(toasts.value[0].type).toBe('error');
    });

    it('should add warning toast', () => {
      const { warning, toasts } = useToast();
      warning('Warning message');
      expect(toasts.value[0].type).toBe('warning');
    });

    it('should add info toast', () => {
      const { info, toasts } = useToast();
      info('Info message');
      expect(toasts.value[0].type).toBe('info');
    });

    it('should respect custom timeout', () => {
      const { success, toasts } = useToast();
      success('Test', { timeout: 5000 });
      expect(toasts.value[0].timeout).toBe(5000);
    });

    it('should respect title option', () => {
      const { success, toasts } = useToast();
      success('Test', { title: 'Custom Title' });
      expect(toasts.value[0].title).toBe('Custom Title');
    });
  });

  describe('dismiss', () => {
    it('should remove a specific toast', () => {
      const { success, dismiss, toasts } = useToast();
      const id = success('Test message');
      expect(toasts.value).toHaveLength(1);
      dismiss(id);
      expect(toasts.value).toHaveLength(0);
    });

    it('should not throw when dismissing non-existent id', () => {
      const { dismiss } = useToast();
      expect(() => dismiss('non-existent')).not.toThrow();
    });
  });

  describe('dismissAll', () => {
    it('should remove all toasts', () => {
      const { success, error, dismissAll, toasts } = useToast();
      success('First');
      error('Second');
      expect(toasts.value).toHaveLength(2);
      dismissAll();
      expect(toasts.value).toHaveLength(0);
    });
  });

  describe('auto-dismiss', () => {
    it('should auto-dismiss after timeout', () => {
      const { success, toasts } = useToast();
      success('Test', { timeout: 1000 });
      expect(toasts.value).toHaveLength(1);
      vi.advanceTimersByTime(2000);
      expect(toasts.value).toHaveLength(0);
    });
  });

  describe('max toasts limit', () => {
    it('should limit toasts to maximum of 5', () => {
      const { success, toasts } = useToast();
      for (let i = 1; i <= 6; i++) {
        success(`Message ${i}`);
      }
      expect(toasts.value.length).toBeLessThanOrEqual(5);
    });
  });

  describe('newer toasts first', () => {
    it('should add new toasts at the beginning', () => {
      const { success, toasts } = useToast();
      success('First');
      success('Second');
      expect(toasts.value[0].message).toBe('Second');
      expect(toasts.value[1].message).toBe('First');
    });
  });
});
