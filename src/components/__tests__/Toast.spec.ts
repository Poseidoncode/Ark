import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import Toast from '../Toast.vue';
import { useToast } from '../../composables/useToast';

describe('Toast', () => {
  beforeEach(() => {
    // Clear any existing toasts
    const { dismissAll } = useToast();
    dismissAll();
  });

  afterEach(() => {
    const { dismissAll } = useToast();
    dismissAll();
  });

  const createWrapper = () => {
    return mount(Toast, {
      global: {
        stubs: {
          TransitionGroup: {
            template: '<div><slot></slot></div>',
          },
        },
      },
    });
  };

  describe('Rendering', () => {
    it('should render toast component', () => {
      const wrapper = createWrapper();
      expect(wrapper.exists()).toBe(true);
    });

    it('should render message when toast is added', async () => {
      const wrapper = createWrapper();
      const { success } = useToast();
      
      success('Test message');
      
      await wrapper.vm.$nextTick();
      
      expect(wrapper.text()).toContain('Test message');
    });

    it('should not render when no toasts exist', () => {
      const wrapper = createWrapper();
      
      // Should not render any toast items
      const toastItems = wrapper.findAll('[role="alert"]');
      expect(toastItems.length).toBe(0);
    });
  });

  describe('Toast Types', () => {
    it('should render success toast with correct styling', async () => {
      const wrapper = createWrapper();
      const { success } = useToast();
      
      success('Success message');
      
      await wrapper.vm.$nextTick();
      
      const toast = wrapper.find('[role="alert"]');
      expect(toast.classes()).toContain('toast-success');
    });

    it('should render error toast with correct styling', async () => {
      const wrapper = createWrapper();
      const { error } = useToast();
      
      error('Error message');
      
      await wrapper.vm.$nextTick();
      
      const toast = wrapper.find('[role="alert"]');
      expect(toast.classes()).toContain('toast-error');
    });

    it('should render warning toast with correct styling', async () => {
      const wrapper = createWrapper();
      const { warning } = useToast();
      
      warning('Warning message');
      
      await wrapper.vm.$nextTick();
      
      const toast = wrapper.find('[role="alert"]');
      expect(toast.classes()).toContain('toast-warning');
    });

    it('should render info toast with correct styling', async () => {
      const wrapper = createWrapper();
      const { info } = useToast();
      
      info('Info message');
      
      await wrapper.vm.$nextTick();
      
      const toast = wrapper.find('[role="alert"]');
      expect(toast.classes()).toContain('toast-info');
    });

    it('should display title when provided', async () => {
      const wrapper = createWrapper();
      const { success } = useToast();
      
      success('Message body', { title: 'Title Here' });
      
      await wrapper.vm.$nextTick();
      
      expect(wrapper.text()).toContain('Title Here');
      expect(wrapper.text()).toContain('Message body');
    });
  });

  describe('Dismiss Functionality', () => {
    it('should have dismiss button', async () => {
      const wrapper = createWrapper();
      const { success } = useToast();
      
      success('Test message');
      
      await wrapper.vm.$nextTick();
      
      const dismissButton = wrapper.find('button[aria-label="Dismiss"]');
      expect(dismissButton.exists()).toBe(true);
    });

    it('should remove toast when dismiss button is clicked', async () => {
      const wrapper = createWrapper();
      const { success, toasts } = useToast();  // ponytail: dismiss unused
      
      success('Test message');
      await wrapper.vm.$nextTick();
      
      expect(toasts.value.length).toBe(1);
      
      const dismissButton = wrapper.find('button[aria-label="Dismiss"]');
      await dismissButton.trigger('click');
      
      await wrapper.vm.$nextTick();
      
      expect(toasts.value.length).toBe(0);
    });

    it('should auto-dismiss after timeout', async () => {
      vi.useFakeTimers();
      
      const wrapper = createWrapper();
      const { success, toasts } = useToast();
      
      // Add toast with short timeout
      success('Test message', { timeout: 1000 });
      
      await wrapper.vm.$nextTick();
      expect(toasts.value.length).toBe(1);
      
      // Advance time past timeout
      vi.advanceTimersByTime(2000);
      
      expect(toasts.value.length).toBe(0);
      
      vi.useRealTimers();
    });

    it('should dismiss all toasts', async () => {
      const wrapper = createWrapper();
      const { success, error, warning, info, dismissAll, toasts } = useToast();
      
      success('Success');
      error('Error');
      warning('Warning');
      info('Info');
      
      await wrapper.vm.$nextTick();
      expect(toasts.value.length).toBe(4);
      
      dismissAll();
      
      await wrapper.vm.$nextTick();
      expect(toasts.value.length).toBe(0);
    });
  });

  describe('Multiple Toasts', () => {
    it('should render multiple toasts', async () => {
      const wrapper = createWrapper();
      const { success, error } = useToast();
      
      success('First message');
      error('Second message');
      
      await wrapper.vm.$nextTick();
      
      const toastItems = wrapper.findAll('[role="alert"]');
      expect(toastItems.length).toBe(2);
    });

    it('should limit toasts to maximum of 5', async () => {
      const wrapper = createWrapper();
      const { success } = useToast();
      
      // Add 6 toasts (should only keep 5)
      for (let i = 1; i <= 6; i++) {
        success(`Message ${i}`);
      }
      
      await wrapper.vm.$nextTick();
      
      const { toasts } = useToast();
      expect(toasts.value.length).toBe(5);
    });
  });

  describe('Icons', () => {
    it('should render success icon for success toast', async () => {
      const wrapper = createWrapper();
      const { success } = useToast();
      
      success('Success');
      
      await wrapper.vm.$nextTick();
      
      // Check for success icon (check path element)
      const icon = wrapper.find('[role="alert"] svg');
      expect(icon.exists()).toBe(true);
    });

    it('should render error icon for error toast', async () => {
      const wrapper = createWrapper();
      const { error } = useToast();
      
      error('Error');
      
      await wrapper.vm.$nextTick();
      
      const icon = wrapper.find('[role="alert"] svg');
      expect(icon.exists()).toBe(true);
    });
  });
});
