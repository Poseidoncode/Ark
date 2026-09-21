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
  /**
   * Queue same-scope calls FIFO instead of deduping/throwing.
   * Required for parameterized mutations (stage [a] vs stage [b]): dedup
   * would silently drop the second call's work, and throwing would surface
   * spurious errors on rapid clicks. Queued calls run strictly one at a time.
   */
  serialize?: boolean;
}

interface LockState {
  owner: string;
  timer: ReturnType<typeof setTimeout> | null;
  /** In-flight task so same-key callers share one promise (dedup). */
  current: Promise<unknown> | null;
}

const DEFAULT_TIMEOUT_MS = 120_000;

const locks = new Map<string, LockState>();
/** FIFO tail per scope for serialized calls. Never rejects. */
const tails = new Map<string, Promise<unknown>>();
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

  /** FIFO path: every same-scope call runs to completion in arrival order. */
  const withLockSerialized = <T>(
    key: string,
    operationName: string,
    fn: () => Promise<T>,
    timeoutMs: number,
  ): Promise<T> => {
    const prevTail = tails.get(key) ?? Promise.resolve();
    let openGate!: () => void;
    const gate = new Promise<void>((resolve) => { openGate = resolve; });
    // This link resolves when the gate opens (settle or timeout-abandon),
    // so abandoning never stalls the rest of the queue.
    const myTail = prevTail.then(
      () => gate,
      () => gate,
    );
    tails.set(key, myTail);

    const gated = (async (): Promise<T> => {
      await prevTail.catch(() => undefined);
      const state: LockState = { owner: operationName, timer: null, current: null };
      locks.set(key, state);
      syncRefs();
      try {
        return await fn();
      } finally {
        if (locks.get(key) === state) {
          locks.delete(key);
          syncRefs();
        }
      }
    })();
    // The task may outlive the waiter's timeout; never let it reject unhandled.
    void gated.then(
      () => undefined,
      () => undefined,
    );

    let timer: ReturnType<typeof setTimeout> | null = null;
    const timeout = new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new OperationTimeoutError(operationName, timeoutMs)), timeoutMs);
    });
    return (async (): Promise<T> => {
      try {
        return await Promise.race([gated, timeout]);
      } finally {
        if (timer) clearTimeout(timer);
        openGate();
        if (tails.get(key) === myTail) tails.delete(key);
      }
    })();
  };

  const withLock = async <T>(operationName: string, fn: () => Promise<T>, options?: LockOptions): Promise<T> => {
    const key = keyOf(options?.scope);
    const timeoutMs = options?.timeoutMs ?? DEFAULT_TIMEOUT_MS;
    if (options?.serialize) {
      return withLockSerialized(key, operationName, fn, timeoutMs);
    }
    const existing = locks.get(key);
    if (existing) {
      // Same-operation in-flight dedup: share the running promise instead of
      // throwing or launching a duplicate. A *different* operation must never
      // receive another operation's result, so it is rejected as blocked.
      if (existing.current && existing.owner === operationName) return existing.current as Promise<T>;
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
    // Timeout releases the scope (liveness: a hung backend call must not brick
    // the UI forever) while the task runs detached. Stale side effects from the
    // detached task are discarded by generation/path guards at the state layer
    // (see repo store), and concurrent git writes fail safely on index.lock.
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

/**
 * Per-repo serialized runner: all mutations for one repository execute
 * strictly one at a time in arrival order. Use this for every repo-scoped
 * git mutation so rapid UI actions queue instead of racing.
 */
export function useRepoLock() {
  const { withLock } = useOperationMutex();
  const withRepoLock = <T>(
    operationName: string,
    repoPath: string | null | undefined,
    fn: () => Promise<T>,
  ): Promise<T> => withLock(operationName, fn, { scope: repoPath ?? 'global', serialize: true });
  return { withRepoLock };
}
