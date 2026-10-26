import type { NavigateFunction } from 'react-router-dom';
import { isDesktopShell } from '../../lib/desktop';

/** Desktop opens code review in the app window; the browser keeps a new tab. */
export function openCodeReview(href: string, navigate: NavigateFunction): void {
  if (isDesktopShell()) {
    const codeReviewFrom = `${window.location.pathname}${window.location.search}${window.location.hash}`;
    navigate(href, { state: { codeReviewFrom } });
    return;
  }
  window.open(href, '_blank', 'noopener,noreferrer');
}

/** Only local application URLs may be used as Code Review back targets. */
export function codeReviewOrigin(state: unknown): string | undefined {
  const from = (state as { codeReviewFrom?: unknown } | null)?.codeReviewFrom;
  if (typeof from !== 'string' || !from.startsWith('/') || from.startsWith('//')) {
    return undefined;
  }
  const base = 'https://coppice.local';
  try {
    if (new URL(from, base).origin !== base) {
      return undefined;
    }
  } catch {
    return undefined;
  }
  return from;
}
