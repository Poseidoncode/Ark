import { ref, type Ref } from 'vue';

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
    } catch (err) {
      if (pending) {
        pending = false;
      }
      throw err;
    } finally {
      executing = false;
      inflight.value = false;
      if (pending) {
        pending = false;
        timer = setTimeout(() => {
          timer = null;
          execute().catch((e) => console.error('Debounced follow-up failed:', e));
        }, 50);
      }
    }
  };

  const trigger = (): Promise<T | undefined> => {
    if (timer) {
      clearTimeout(timer);
    }
    return new Promise((resolve, reject) => {
      timer = setTimeout(() => {
        timer = null;
        execute().then(resolve, reject);
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
