import { onMounted, onUnmounted } from 'vue';

export interface KeyboardShortcut {
  key: string;
  ctrl?: boolean;
  alt?: boolean;
  shift?: boolean;
  meta?: boolean;
  action: () => void;
  description?: string;
}

const registeredShortcuts: KeyboardShortcut[] = [];

export function useKeyboardShortcuts(shortcuts: KeyboardShortcut[]) {
  const handleKeydown = (event: KeyboardEvent) => {
    // Ignore if typing in input/textarea
    const target = event.target as HTMLElement;
    if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable) {
      return;
    }

    for (const shortcut of shortcuts) {
      const keyMatch = event.key.toLowerCase() === shortcut.key.toLowerCase();
      const ctrlMatch = shortcut.ctrl ? (event.ctrlKey || event.metaKey) : !event.ctrlKey && !event.metaKey;
      const altMatch = shortcut.alt ? event.altKey : !event.altKey;
      const shiftMatch = shortcut.shift ? event.shiftKey : !event.shiftKey;

      if (keyMatch && ctrlMatch && altMatch && shiftMatch) {
        event.preventDefault();
        event.stopPropagation();
        shortcut.action();
        break;
      }
    }
  };

  onMounted(() => {
    registeredShortcuts.push(...shortcuts);
    window.addEventListener('keydown', handleKeydown);
  });

  onUnmounted(() => {
    for (const shortcut of shortcuts) {
      const index = registeredShortcuts.indexOf(shortcut);
      if (index > -1) {
        registeredShortcuts.splice(index, 1);
      }
    }
    window.removeEventListener('keydown', handleKeydown);
  });

  return {
    shortcuts
  };
}

export function getRegisteredShortcuts(): KeyboardShortcut[] {
  return [...registeredShortcuts];
}
