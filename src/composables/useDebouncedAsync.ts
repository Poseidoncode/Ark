import { ref, type Ref } from 'vue';

/**
 * Creates a debounced version of an async function.
 *
 * Multiple rapid calls within the delay window collapse into a single
 * invocation. If a call is already in-flight, the latest call is queued
 * and executed after the current one completes, ensuring the most
 * recent state is always fetched.
 *
 * @param fn - The async function to debounce
 * @param delay - Debounce window in milliseconds
 */
export function useDebouncedAsync<T>(
  fn: () => Promise<T>,
  delay: number = 300,
): { trigger: () => Promise<T | undefined>; inflight: Ref<boolean>; cancel: () => void } {
  let timer: ReturnType<typeof setTimeout> | null = null;
  let pending = false;
  let executing = false;
  const inflight = ref(false);

  const execute = async (): Promise<T | undefined> => {
    if (executing) {
      pending = true;
      return undefined;
    }
    executing = true;
    inflight.value = true;
    try {
      return await fn();
    } finally {
      executing = false;
      inflight.value = false;
      if (pending) {
        pending = false;
        // Small delay to batch the pending call
        timer = setTimeout(() => {
          timer = null;
          execute();
        }, 50);
      }
    }
  };

  const trigger = (): Promise<T | undefined> => {
    if (timer) {
      clearTimeout(timer);
    }
    return new Promise((resolve) => {
      timer = setTimeout(() => {
        timer = null;
        execute().then(resolve);
      }, delay);
    });
  };

  const cancel = (): void => {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
    pending = false;
  };

  return { trigger, inflight, cancel };
}
