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

  it('rejects a different operation instead of sharing the wrong result', async () => {
    release('repo-x');
    const slow = withLock(
      'first',
      () => new Promise<string>((r) => setTimeout(() => r('done'), 30)),
      { scope: 'repo-x' },
    );

    await expect(
      withLock('second', async () => 'nope', { scope: 'repo-x' }),
    ).rejects.toThrow('blocked by "first"');

    await expect(slow).resolves.toBe('done');
  });

  it('runs serialized calls strictly in arrival order', async () => {
    const order: string[] = [];
    const mk = (name: string, ms: number) => () =>
      new Promise<string>((r) => setTimeout(() => { order.push(name); r(name); }, ms));

    const [a, b, c] = await Promise.all([
      withLock('stage-a', mk('a', 30), { scope: 'repo-q', serialize: true }),
      withLock('stage-b', mk('b', 5), { scope: 'repo-q', serialize: true }),
      withLock('stage-c', mk('c', 5), { scope: 'repo-q', serialize: true }),
    ]);

    expect([a, b, c]).toEqual(['a', 'b', 'c']);
    expect(order).toEqual(['a', 'b', 'c']);
  });

  it('keeps the serialized queue moving after a failure', async () => {
    const fail = withLock(
      'bad',
      async () => { throw new Error('boom'); },
      { scope: 'repo-q2', serialize: true },
    );
    const ok = withLock('good', async () => 'fine', { scope: 'repo-q2', serialize: true });

    await expect(fail).rejects.toThrow('boom');
    await expect(ok).resolves.toBe('fine');
  });

  it('unblocks the serialized queue on timeout', async () => {
    const hung = withLock('hung', () => new Promise<string>(() => {}), {
      scope: 'repo-q3',
      serialize: true,
      timeoutMs: 30,
    });
    const next = withLock('next', async () => 'ok', {
      scope: 'repo-q3',
      serialize: true,
      timeoutMs: 1000,
    });

    await expect(hung).rejects.toBeInstanceOf(OperationTimeoutError);
    await expect(next).resolves.toBe('ok');
  });
});
