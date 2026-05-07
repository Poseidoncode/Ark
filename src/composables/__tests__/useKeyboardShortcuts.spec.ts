import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { defineComponent, h } from 'vue';
import { useKeyboardShortcuts, getRegisteredShortcuts } from '../useKeyboardShortcuts';

// Helper to simulate keydown event
function simulateKeydown(key: string, options: { ctrlKey?: boolean; metaKey?: boolean; altKey?: boolean; shiftKey?: boolean; target?: EventTarget } = {}) {
  const event = new KeyboardEvent('keydown', {
    key,
    ctrlKey: options.ctrlKey ?? false,
    metaKey: options.metaKey ?? false,
    altKey: options.altKey ?? false,
    shiftKey: options.shiftKey ?? false,
    bubbles: true,
  });
  
  // Mock preventDefault and stopPropagation
  vi.spyOn(event, 'preventDefault');
  vi.spyOn(event, 'stopPropagation');
  
  // Set the target if provided
  if (options.target) {
    Object.defineProperty(event, 'target', { value: options.target, writable: false });
  }
  
  window.dispatchEvent(event);
  return event;
}

// Helper to create a mock document body for input detection
function setupInputDetection() {
  Object.defineProperty(HTMLElement.prototype, 'isContentEditable', {
    get: function() {
      return this.getAttribute('contenteditable') === 'true';
    },
    configurable: true,
  });
}

describe('useKeyboardShortcuts', () => {
  beforeEach(() => {
    setupInputDetection();
    // Clear registered shortcuts
    getRegisteredShortcuts().length = 0;
  });

  it('should fire shortcut with ctrl: true only when Ctrl is pressed', async () => {
    const actionSpy = vi.fn();
    
    const TestComponent = defineComponent({
      setup() {
        useKeyboardShortcuts([
          { key: 's', ctrl: true, action: actionSpy }
        ]);
        return () => h('div');
      }
    });

    const wrapper = mount(TestComponent);

    // Should NOT fire without Ctrl
    simulateKeydown('s', { ctrlKey: false });
    expect(actionSpy).not.toHaveBeenCalled();

    // Should fire with Ctrl
    simulateKeydown('s', { ctrlKey: true });
    expect(actionSpy).toHaveBeenCalledTimes(1);

    // Should fire with Meta (Cmd on Mac)
    simulateKeydown('s', { metaKey: true });
    expect(actionSpy).toHaveBeenCalledTimes(2);

    wrapper.unmount();
  });

  it('should fire shortcut with ctrl: false even when Ctrl is held (BUG FIX)', async () => {
    const actionSpy = vi.fn();
    
    const TestComponent = defineComponent({
      setup() {
        useKeyboardShortcuts([
          { key: 's', ctrl: false, action: actionSpy }
        ]);
        return () => h('div');
      }
    });

    const wrapper = mount(TestComponent);

    // Should fire without any modifiers
    simulateKeydown('s', { ctrlKey: false });
    expect(actionSpy).toHaveBeenCalledTimes(1);

    // Should ALSO fire when Ctrl is held (this was the bug!)
    // ctrl: false means "Ctrl not required", not "Ctrl must be absent"
    simulateKeydown('s', { ctrlKey: true });
    expect(actionSpy).toHaveBeenCalledTimes(2);

    // Should ALSO fire when Meta is held
    simulateKeydown('s', { metaKey: true });
    expect(actionSpy).toHaveBeenCalledTimes(3);

    wrapper.unmount();
  });

  it('should fire shortcut without ctrl property (defaults to false behavior)', async () => {
    const actionSpy = vi.fn();
    
    const TestComponent = defineComponent({
      setup() {
        useKeyboardShortcuts([
          { key: 'k', action: actionSpy }  // no ctrl property
        ]);
        return () => h('div');
      }
    });

    const wrapper = mount(TestComponent);

    // Should fire without modifiers
    simulateKeydown('k', { ctrlKey: false });
    expect(actionSpy).toHaveBeenCalledTimes(1);

    // Should also fire with Ctrl (same as ctrl: false)
    simulateKeydown('k', { ctrlKey: true });
    expect(actionSpy).toHaveBeenCalledTimes(2);

    wrapper.unmount();
  });

  it('should ignore shortcuts when typing in input', async () => {
    const actionSpy = vi.fn();
    
    const TestComponent = defineComponent({
      setup() {
        useKeyboardShortcuts([
          { key: 's', action: actionSpy }
        ]);
        return () => h('div');
      }
    });

    const wrapper = mount(TestComponent);
    
    // Create an input element and focus it
    const input = document.createElement('input');
    input.type = 'text';
    document.body.appendChild(input);
    input.focus();

    // Should NOT fire when typing in input - dispatch event with input as target
    simulateKeydown('s', { ctrlKey: false, target: input });
    expect(actionSpy).not.toHaveBeenCalled();

    document.body.removeChild(input);
    wrapper.unmount();
  });

  it('should ignore shortcuts when typing in textarea', async () => {
    const actionSpy = vi.fn();
    
    const TestComponent = defineComponent({
      setup() {
        useKeyboardShortcuts([
          { key: 's', action: actionSpy }
        ]);
        return () => h('div');
      }
    });

    const wrapper = mount(TestComponent);
    
    const textarea = document.createElement('textarea');
    document.body.appendChild(textarea);
    textarea.focus();

    // Should NOT fire when typing in textarea - dispatch event with textarea as target
    simulateKeydown('s', { ctrlKey: false, target: textarea });
    expect(actionSpy).not.toHaveBeenCalled();

    document.body.removeChild(textarea);
    wrapper.unmount();
  });

  it('should handle case-insensitive key matching', async () => {
    const actionSpy = vi.fn();
    
    const TestComponent = defineComponent({
      setup() {
        useKeyboardShortcuts([
          { key: 'S', ctrl: false, action: actionSpy }
        ]);
        return () => h('div');
      }
    });

    const wrapper = mount(TestComponent);

    // Should match lowercase 's'
    simulateKeydown('s', { ctrlKey: false });
    expect(actionSpy).toHaveBeenCalledTimes(1);

    // Should match uppercase 'S'
    simulateKeydown('S', { ctrlKey: false });
    expect(actionSpy).toHaveBeenCalledTimes(2);

    wrapper.unmount();
  });

  it('should prevent default and stop propagation on matched shortcut', async () => {
    const actionSpy = vi.fn();
    
    const TestComponent = defineComponent({
      setup() {
        useKeyboardShortcuts([
          { key: 's', ctrl: false, action: actionSpy }
        ]);
        return () => h('div');
      }
    });

    const wrapper = mount(TestComponent);
    
    const event = simulateKeydown('s', { ctrlKey: false });
    
    expect(actionSpy).toHaveBeenCalled();
    expect(event.preventDefault).toHaveBeenCalled();
    expect(event.stopPropagation).toHaveBeenCalled();

    wrapper.unmount();
  });
});
