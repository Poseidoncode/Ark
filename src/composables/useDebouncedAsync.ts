import { ref, type Ref } from 'vue';

export class DebounceCancelledError extends Error {
  constructor() {
    super('Debounced operation cancelled');
    this.name = 'DebounceCancelledError';
  }
}

type Waiter<T> = {
  resolve: (value: T) => void;
  reject: (reason: unknown) => void;
};

export function useDebouncedAsync<T>(
  fn: () => Promise<T>,
  delay: number = 300,
): { trigger: () => Promise<T>; inflight: Ref<boolean>; cancel: () => void } {
  let timer: ReturnType<typeof setTimeout> | null = null;
  let executing = false;
  // Callers waiting on the armed (not yet fired) cycle.
  let waiters: Waiter<T>[] = [];
  // Callers that arrived mid-flight. They merge into one trailing run and
  // settle with its result — never with `undefined`, which would silently
  // drop their update.
  let trailing: Waiter<T>[] = [];
  const inflight = ref(false);

  const settle = (list: Waiter<T>[], kind: 'resolve' | 'reject', value: unknown): void => {
    const batch = list.splice(0, list.length);
    for (const w of batch) {
      if (kind === 'resolve') w.resolve(value as T);
      else w.reject(value);
    }
  };

  const run = async (): Promise<void> => {
    executing = true;
    inflight.value = true;
    try {
      const result = await fn();
      settle(waiters, 'resolve', result);
    } catch (err) {
      settle(waiters, 'reject', err);
    } finally {
      executing = false;
      inflight.value = false;
      if (trailing.length > 0) {
        // Trailing edge: one merged re-run serves every mid-flight caller.
        waiters = trailing;
        trailing = [];
        timer = setTimeout(() => {
          timer = null;
          void run();
        }, 50);
      }
    }
  };

  const trigger = (): Promise<T> => {
    return new Promise<T>((resolve, reject) => {
      const waiter: Waiter<T> = { resolve, reject };
      if (executing) {
        trailing.push(waiter);
        return;
      }
      waiters.push(waiter);
      if (timer) {
        clearTimeout(timer);
      }
      timer = setTimeout(() => {
        timer = null;
        void run();
      }, delay);
    });
  };

  const cancel = (): void => {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
    // Settle every pending trigger promise instead of leaving it hanging or
    // resolving it with `undefined`. The in-flight run (if any) is left to
    // settle its own waiters with the real result.
    const err = new DebounceCancelledError();
    settle(waiters, 'reject', err);
    settle(trailing, 'reject', err);
  };

  return { trigger, inflight, cancel };
}
