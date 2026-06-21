import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import { charsForFrame, useTypewriter } from './useTypewriter';

describe('charsForFrame', () => {
  it('returns 0 when nothing remains', () => {
    expect(charsForFrame(0, 2, 24)).toBe(0);
    expect(charsForFrame(-5, 2, 24)).toBe(0);
  });

  it('scales with backlog size so large backlogs drain faster', () => {
    const small = charsForFrame(10, 2, 24);
    const large = charsForFrame(1000, 2, 24);
    expect(large).toBeGreaterThan(small);
    expect(small).toBe(2 + Math.ceil(10 / 24));
    expect(large).toBe(2 + Math.ceil(1000 / 24));
  });

  it('drains everything when the divisor is non-positive', () => {
    expect(charsForFrame(50, 2, 0)).toBe(50);
  });
});

function mockMatchMedia(matches: (query: string) => boolean) {
  window.matchMedia = vi.fn().mockImplementation((query: string) => ({
    matches: matches(query),
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  }));
}

describe('useTypewriter', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mockMatchMedia(() => false);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('returns the full text immediately when disabled', () => {
    const { result } = renderHook(() =>
      useTypewriter('hello world', { enabled: false }),
    );
    expect(result.current).toBe('hello world');
  });

  it('returns the full text immediately when reduced motion is preferred', () => {
    mockMatchMedia((query) => query.includes('reduce'));
    const { result } = renderHook(() =>
      useTypewriter('hello world', { enabled: true }),
    );
    expect(result.current).toBe('hello world');
  });

  it('skips animation for text at or above maxAnimateLength', () => {
    const big = 'x'.repeat(6000);
    const { result } = renderHook(() =>
      useTypewriter(big, { enabled: true, maxAnimateLength: 6000 }),
    );
    expect(result.current).toBe(big);
  });

  it('progressively reveals text and reaches the full string', () => {
    const full = 'a'.repeat(500);
    const { result } = renderHook(() =>
      useTypewriter(full, { enabled: true, commitIntervalMs: 0 }),
    );

    expect(result.current).toBe('');

    act(() => {
      vi.advanceTimersByTime(48);
    });
    const midLength = result.current.length;
    expect(midLength).toBeGreaterThan(0);
    expect(midLength).toBeLessThan(full.length);

    act(() => {
      vi.advanceTimersByTime(5000);
    });
    expect(result.current).toBe(full);
  });

  it('drains a large backlog faster per frame than a small one', () => {
    const short = 'b'.repeat(50);
    const long = 'c'.repeat(2000);
    const shortHook = renderHook(() =>
      useTypewriter(short, {
        enabled: true,
        commitIntervalMs: 0,
        backlogAccelDivisor: 10,
      }),
    );
    const longHook = renderHook(() =>
      useTypewriter(long, {
        enabled: true,
        commitIntervalMs: 0,
        backlogAccelDivisor: 10,
      }),
    );

    act(() => {
      vi.advanceTimersByTime(48);
    });

    expect(longHook.result.current.length).toBeGreaterThan(
      shortHook.result.current.length,
    );
  });

  it('flushes to full when disabled mid-stream', () => {
    const full = 'd'.repeat(500);
    const { result, rerender } = renderHook(
      ({ enabled }: { enabled: boolean }) =>
        useTypewriter(full, { enabled, commitIntervalMs: 0 }),
      { initialProps: { enabled: true } },
    );

    act(() => {
      vi.advanceTimersByTime(32);
    });
    expect(result.current.length).toBeGreaterThan(0);
    expect(result.current.length).toBeLessThan(full.length);

    rerender({ enabled: false });
    expect(result.current).toBe(full);
  });

  it('flushes to full when the document becomes hidden', () => {
    const full = 'e'.repeat(500);
    const { result } = renderHook(() =>
      useTypewriter(full, { enabled: true, commitIntervalMs: 0 }),
    );

    act(() => {
      vi.advanceTimersByTime(32);
    });
    expect(result.current.length).toBeLessThan(full.length);

    const hiddenSpy = vi
      .spyOn(document, 'hidden', 'get')
      .mockReturnValue(true);
    act(() => {
      document.dispatchEvent(new Event('visibilitychange'));
    });
    hiddenSpy.mockRestore();

    expect(result.current).toBe(full);
  });
});
