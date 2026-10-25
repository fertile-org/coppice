import type { NavigateFunction } from 'react-router-dom';
import { isDesktopShell } from '../../lib/desktop';

/** Desktop opens code review in the app window; the browser keeps a new tab. */
export function openCodeReview(href: string, navigate: NavigateFunction): void {
  if (isDesktopShell()) {
    navigate(href);
    return;
  }
  window.open(href, '_blank', 'noopener,noreferrer');
}
