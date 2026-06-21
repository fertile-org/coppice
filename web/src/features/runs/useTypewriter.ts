import { useEffect, useRef, useState } from 'react';

export interface UseTypewriterOptions {
  /**
   * Reveal progressively when true; show the full text instantly when false.
   * Pass true only for the live tail entry so historical/finished content
   * never animates.
   */
  enabled: boolean;
  /** Baseline characters revealed per frame for a small backlog. */
  baseCharsPerFrame?: number;
  /** Larger divisor = weaker acceleration. chars += ceil(remaining / divisor). */
  backlogAccelDivisor?: number;
  /** Minimum ms between React state commits (bounds markdown re-parse cost). */
  commitIntervalMs?: number;
  /** Texts at or above this length skip animation entirely (bounds cost). */
  maxAnimateLength?: number;
}

export const TYPEWRITER_DEFAULTS = {
  baseCharsPerFrame: 2,
  backlogAccelDivisor: 24,
  commitIntervalMs: 40,
  maxAnimateLength: 6000,
} as const;

/**
 * Characters to reveal on a single frame given the current backlog.
 *
 * Base pace plus a term proportional to remaining text, so a large backlog
 * drains visibly faster than a small one without ever stalling. This is the
 * pure core of the adaptive drain rate and is unit-tested directly.
 */
export function charsForFrame(
  remaining: number,
  baseCharsPerFrame: number,
  backlogAccelDivisor: number,
): number {
  if (remaining <= 0) return 0;
  if (backlogAccelDivisor <= 0) return remaining;
  return baseCharsPerFrame + Math.ceil(remaining / backlogAccelDivisor);
}

function readReducedMotion(): boolean {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
    return false;
  }
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

function usePrefersReducedMotion(): boolean {
  const [reduced, setReduced] = useState<boolean>(readReducedMotion);
  useEffect(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
      return;
    }
    const mql = window.matchMedia('(prefers-reduced-motion: reduce)');
    const onChange = () => setReduced(mql.matches);
    mql.addEventListener('change', onChange);
    return () => mql.removeEventListener('change', onChange);
  }, []);
  return reduced;
}

/**
 * Reveals `fullText` character-by-character at a smooth, adaptive cadence.
 *
 * Contract:
 * - Only animates when `enabled` is true (i.e. the entry is the live tail).
 * - Shows the full text instantly when disabled, when the document becomes
 *   hidden (no catch-up jump on return), when the user prefers reduced motion,
 *   or when the text is at/above `maxAnimateLength`. The disabled/reduced/
 *   oversized cases are handled purely in render; the hidden case flushes via
 *   the visibility listener.
 * - Appended `fullText` keeps the current reveal position and animates only
 *   the new tail. A non-prefix replacement restarts from 0 so reused
 *   components do not leak old text into unrelated content.
 */
export function useTypewriter(fullText: string, options: UseTypewriterOptions): string {
  const {
    enabled,
    baseCharsPerFrame = TYPEWRITER_DEFAULTS.baseCharsPerFrame,
    backlogAccelDivisor = TYPEWRITER_DEFAULTS.backlogAccelDivisor,
    commitIntervalMs = TYPEWRITER_DEFAULTS.commitIntervalMs,
    maxAnimateLength = TYPEWRITER_DEFAULTS.maxAnimateLength,
  } = options;

  const reducedMotion = usePrefersReducedMotion();

  const shouldAnimate =
    enabled &&
    fullText.length > 0 &&
    fullText.length < maxAnimateLength &&
    !reducedMotion;

  const [revealedCount, setRevealedCount] = useState(0);
  const countRef = useRef(0);
  const lastCommitRef = useRef(0);
  const frameRef = useRef<number | null>(null);
  const previousFullTextRef = useRef('');

  useEffect(() => {
    if (!shouldAnimate) {
      previousFullTextRef.current = fullText;
      countRef.current = fullText.length;
      lastCommitRef.current = 0;
      setRevealedCount(fullText.length);
      return;
    }

    const previousFullText = previousFullTextRef.current;
    const isAppend =
      previousFullText.length > 0 && fullText.startsWith(previousFullText);
    const initialCount = isAppend
      ? Math.min(countRef.current, fullText.length)
      : 0;

    previousFullTextRef.current = fullText;
    countRef.current = initialCount;
    lastCommitRef.current = 0;
    setRevealedCount(initialCount);

    let cancelled = false;

    const cancelFrame = () => {
      if (frameRef.current !== null) {
        cancelAnimationFrame(frameRef.current);
        frameRef.current = null;
      }
    };

    const step = (now: number) => {
      if (cancelled) return;
      frameRef.current = null;
      if (lastCommitRef.current === 0) lastCommitRef.current = now;

      const remaining = fullText.length - countRef.current;
      if (remaining <= 0) return;

      const advance = charsForFrame(remaining, baseCharsPerFrame, backlogAccelDivisor);
      countRef.current = Math.min(fullText.length, countRef.current + advance);

      // Throttle React commits so large markdown blocks are not re-parsed on
      // every animation frame. Always commit on the final frame.
      const elapsed = now - lastCommitRef.current;
      if (elapsed >= commitIntervalMs || countRef.current === fullText.length) {
        setRevealedCount(countRef.current);
        lastCommitRef.current = now;
      }

      if (countRef.current < fullText.length) {
        frameRef.current = requestAnimationFrame(step);
      }
    };

    frameRef.current = requestAnimationFrame(step);

    // Flush on tab hide so returning to the tab never produces a giant jump;
    // rAF would stall while hidden anyway, so draining instantly is correct.
    const onVisibility = () => {
      if (document.hidden) {
        cancelled = true;
        cancelFrame();
        countRef.current = fullText.length;
        setRevealedCount(fullText.length);
      }
    };
    document.addEventListener('visibilitychange', onVisibility);

    return () => {
      cancelled = true;
      cancelFrame();
      document.removeEventListener('visibilitychange', onVisibility);
    };
  }, [shouldAnimate, fullText, baseCharsPerFrame, backlogAccelDivisor, commitIntervalMs]);

  if (!shouldAnimate) return fullText;
  return fullText.slice(0, revealedCount);
}
