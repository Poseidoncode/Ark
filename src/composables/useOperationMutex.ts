import { ref } from 'vue';

export class OperationTimeoutError extends Error {
  constructor(operationName: string, timeoutMs: number) {
    super(`Operation "${operationName}" timed out after ${timeoutMs}ms`);
    this.name = 'OperationTimeoutError';
  }
}

export interface LockOptions {
  /** Lock scope. Pass the repo path for per-repo locks; defaults to 'global'. */
  scope?: string;
  /** Auto-release + reject after this long. Defaults to 120s. */
  timeoutMs?: number;
}

interface LockState {
  owner: string;
  timer: ReturnType<typeof setTimeout> | null;
  /** In-flight task so same-key callers share one promise (dedup). */
  current: Promise<unknown> | null;
}

const DEFAULT_TIMEOUT_MS = 120_000;

const locks = new Map<string, LockState>();
const isOperating = ref(false);
const currentOperation = ref<string | null>(null);

const keyOf = (scope?: string): string => scope ?? 'global';

const syncRefs = (): void => {
  isOperating.value = locks.size > 0;
  const first = locks.values().next().value as LockState | undefined;
  currentOperation.value = first ? first.owner : null;
};

const clearTimer = (state: LockState): void => {
  if (state.timer) {
    clearTimeout(state.timer);
    state.timer = null;
  }
};

export function useOperationMutex() {
  const acquire = (operationName: string, scope?: string, timeoutMs: number = DEFAULT_TIMEOUT_MS): boolean => {
    const key = keyOf(scope);
    if (locks.has(key)) {
      console.warn(`Operation "${operationName}" blocked: "${locks.get(key)?.owner}" is in progress (scope "${key}")`);
      return false;
    }
    const state: LockState = { owner: operationName, timer: null, current: null };
    state.timer = setTimeout(() => {
      if (locks.get(key) === state) {
        locks.delete(key);
        syncRefs();
        console.warn(`Operation "${operationName}" auto-released after ${timeoutMs}ms (scope "${key}")`);
      }
    }, timeoutMs);
    locks.set(key, state);
    syncRefs();
    return true;
  };

  const release = (scope?: string) => {
    const key = keyOf(scope);
    const state = locks.get(key);
    if (!state) return;
    clearTimer(state);
    locks.delete(key);
    syncRefs();
  };

  const withLock = async <T>(operationName: string, fn: () => Promise<T>, options?: LockOptions): Promise<T> => {
    const key = keyOf(options?.scope);
    const timeoutMs = options?.timeoutMs ?? DEFAULT_TIMEOUT_MS;
    const existing = locks.get(key);
    if (existing) {
      // Same-key in-flight dedup: share the running promise instead of
      // throwing or launching a duplicate operation.
      if (existing.current) return existing.current as Promise<T>;
      throw new Error(`Operation "${operationName}" blocked by "${existing.owner}"`);
    }
    const state: LockState = { owner: operationName, timer: null, current: null };
    locks.set(key, state);
    syncRefs();
    let task!: Promise<T>;
    task = (async (): Promise<T> => {
      try {
        return await fn();
      } finally {
        clearTimer(state);
        if (locks.get(key) === state) {
          locks.delete(key);
          syncRefs();
        }
      }
    })();
    state.current = task;
    let rejectTimeout!: (reason: unknown) => void;
    const timeout = new Promise<never>((_, reject) => {
      rejectTimeout = reject;
    });
    // Single timeout timer: auto-release the lock and reject the waiter so a
    // hung operation can never hold its scope forever.
    state.timer = setTimeout(() => {
      if (locks.get(key) === state) {
        locks.delete(key);
        syncRefs();
      }
      rejectTimeout(new OperationTimeoutError(operationName, timeoutMs));
    }, timeoutMs);
    try {
      return await Promise.race([task, timeout]);
    } finally {
      // If the timeout won, the task's finally still releases safely via the
      // identity check once the underlying operation settles.
      void task.then(
        () => undefined,
        () => undefined,
      );
    }
  };

  return {
    isOperating,
    currentOperation,
    acquire,
    release,
    withLock,
  };
}
