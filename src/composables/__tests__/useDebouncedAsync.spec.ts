import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { useDebouncedAsync, DebounceCancelledError } from '../useDebouncedAsync';

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('useDebouncedAsync', () => {
  it('merges rapid triggers into one run, resolving all with the real value', async () => {
    const fn = vi.fn().mockResolvedValue('ok');
    const { trigger } = useDebouncedAsync(fn, 300);

    const p1 = trigger();
    const p2 = trigger();
    const p3 = trigger();
    expect(fn).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(300);

    await expect(p1).resolves.toBe('ok');
    await expect(p2).resolves.toBe('ok');
    await expect(p3).resolves.toBe('ok');
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('serves mid-flight triggers with a trailing run instead of undefined', async () => {
    let calls = 0;
    const fn = vi.fn().mockImplementation(async () => {
      calls++;
      const n = calls;
      await new Promise((r) => setTimeout(r, 100));
      return `v${n}`;
    });
    const { trigger } = useDebouncedAsync(fn, 300);

    const p1 = trigger();
    await vi.advanceTimersByTimeAsync(300); // first run starts
    const p2 = trigger(); // arrives mid-flight
    await vi.advanceTimersByTimeAsync(100); // first run settles
    await vi.advanceTimersByTimeAsync(50); // trailing run fires
    await vi.advanceTimersByTimeAsync(100); // trailing run settles

    await expect(p1).resolves.toBe('v1');
    await expect(p2).resolves.toBe('v2');
    expect(p2).not.toBe(p1);
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it('cancel settles pending triggers with a rejection instead of hanging', async () => {
    const fn = vi.fn().mockResolvedValue('ok');
    const { trigger, cancel } = useDebouncedAsync(fn, 300);

    const p = trigger();
    const assertion = expect(p).rejects.toBeInstanceOf(DebounceCancelledError);
    cancel();
    await assertion;
    expect(fn).not.toHaveBeenCalled();
  });
});
