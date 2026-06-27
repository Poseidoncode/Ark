import { ref } from 'vue';

const isOperating = ref(false);
const currentOperation = ref<string | null>(null);

export function useOperationMutex() {
  const acquire = (operationName: string): boolean => {
    if (isOperating.value) {
      console.warn(`Operation "${operationName}" blocked: "${currentOperation.value}" is in progress`);
      return false;
    }
    isOperating.value = true;
    currentOperation.value = operationName;
    return true;
  };

  const release = () => {
    isOperating.value = false;
    currentOperation.value = null;
  };

  const withLock = async <T>(operationName: string, fn: () => Promise<T>): Promise<T> => {
    if (!acquire(operationName)) {
      throw new Error(`Operation "${operationName}" blocked by "${currentOperation.value}"`);
    }
    try {
      return await fn();
    } finally {
      release();
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
