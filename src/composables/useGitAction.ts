import { useUIStore } from '../stores/ui';
import { useRepoStore } from '../stores/repo';
import { useToast } from './useToast';

/**
 * Composable that encapsulates the repetitive try/catch/finally pattern
 * used across components for Git operations.
 *
 * Handles loading state, error display, retry registration, and repo refresh
 * in a single reusable function.
 */
export function useGitAction() {
  const uiStore = useUIStore();
  const repoStore = useRepoStore();
  const toast = useToast();

  /**
   * Execute a Git operation with standardized loading, error handling,
   * optional success toast, and optional repo refresh.
   *
   * @param op - Operation name for the mutex lock
   * @param fn - The async operation to execute
   * @param options - Configuration for loading message, toast, refresh, etc.
   */
  async function executeGitAction<T>(
    fn: () => Promise<T>,
    options: {
      loadingMessage?: string;
      majorOperation?: boolean;
      successMessage?: string;
      successTitle?: string;
      refreshRepo?: boolean;
      retryFn?: () => Promise<void>;
    } = {},
  ): Promise<T | undefined> {
    const {
      loadingMessage = '',
      majorOperation = false,
      successMessage,
      successTitle = 'Success',
      refreshRepo = true,
      retryFn,
    } = options;

    try {
      uiStore.setLoading(true, loadingMessage, majorOperation);
      uiStore.clearError();
      const result = await fn();
      if (successMessage) {
        toast.success(successMessage, { title: successTitle });
      }
      if (refreshRepo) {
        await repoStore.refreshRepo();
      }
      uiStore.clearError();
      return result;
    } catch (err) {
      const errMsg = err instanceof Error ? err.message : String(err);
      uiStore.setError(errMsg);
      if (retryFn) {
        uiStore.lastFailedOperation = retryFn;
      }
      return undefined;
    } finally {
      uiStore.setLoading(false);
    }
  }

  return {
    executeGitAction,
  };
}
