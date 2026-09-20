import { describe, it, expect, vi } from 'vitest';
import { useOperationMutex, OperationTimeoutError } from '../useOperationMutex';

const { withLock, release } = useOperationMutex();

describe('useOperationMutex', () => {
  it('shares one promise for same-scope concurrent calls', async () => {
    release('repo-a');
    const fn = vi.fn().mockImplementation(
      () => new Promise((r) => setTimeout(() => r('done'), 20)),
    );

    const [a, b] = await Promise.all([
      withLock('op', fn, { scope: 'repo-a' }),
      withLock('op', fn, { scope: 'repo-a' }),
    ]);

    expect(a).toBe('done');
    expect(b).toBe('done');
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('runs different scopes concurrently', async () => {
    release('repo-a');
    release('repo-b');
    const fa = vi.fn().mockImplementation(
      () => new Promise((r) => setTimeout(() => r('a'), 20)),
    );
    const fb = vi.fn().mockImplementation(
      () => new Promise((r) => setTimeout(() => r('b'), 10)),
    );

    const [a, b] = await Promise.all([
      withLock('op-a', fa, { scope: 'repo-a' }),
      withLock('op-b', fb, { scope: 'repo-b' }),
    ]);

    expect(a).toBe('a');
    expect(b).toBe('b');
    expect(fa).toHaveBeenCalledTimes(1);
    expect(fb).toHaveBeenCalledTimes(1);
  });

  it('rejects on timeout and releases the scope', async () => {
    release('repo-t');
    const never = () => new Promise<string>(() => {});

    await expect(
      withLock('hung', never, { scope: 'repo-t', timeoutMs: 50 }),
    ).rejects.toBeInstanceOf(OperationTimeoutError);

    await expect(
      withLock('next', async () => 'ok', { scope: 'repo-t', timeoutMs: 1000 }),
    ).resolves.toBe('ok');
  });
});
