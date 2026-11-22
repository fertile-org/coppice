import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import { charsForFrame, useTypewriter } from './useTypewriter';

describe('charsForFrame', () => {
  it('scales reveal rate with backlog and edge divisors', () => {
    expect(charsForFrame(0, 2, 24)).toBe(0);
    expect(charsForFrame(1000, 2, 24)).toBeGreaterThan(charsForFrame(10, 2, 24));
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

  it('returns full text when animation is skipped', () => {
    const text = 'hello world';
    const { result: disabled } = renderHook(() => useTypewriter(text, { enabled: false }));
    expect(disabled.current).toBe(text);

    mockMatchMedia((query) => query.includes('reduce'));
    const { result: reduced } = renderHook(() => useTypewriter(text, { enabled: true }));
    expect(reduced.current).toBe(text);

    const long = 'x'.repeat(6000);
    const { result: capped } = renderHook(() =>
      useTypewriter(long, { enabled: true, maxAnimateLength: 6000 }),
    );
    expect(capped.current).toBe(long);
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

  it('continues from the current reveal position when streaming text appends', () => {
    const { result, rerender } = renderHook(
      ({ full }: { full: string }) =>
        useTypewriter(full, { enabled: true, commitIntervalMs: 0 }),
      { initialProps: { full: 'hello world'.repeat(20) } },
    );

    act(() => {
      vi.advanceTimersByTime(64);
    });
    const beforeAppend = result.current;
    expect(beforeAppend.length).toBeGreaterThan(0);
    expect(beforeAppend.length).toBeLessThan('hello world'.repeat(20).length);

    rerender({ full: 'hello world'.repeat(20) + ' appended tail' });

    expect(result.current).toBe(beforeAppend);

    act(() => {
      vi.advanceTimersByTime(5000);
    });
    expect(result.current).toBe('hello world'.repeat(20) + ' appended tail');
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
