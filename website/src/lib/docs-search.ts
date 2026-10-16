/** Content-locked docs search strings. Placeholder and accessible label match. */
export const SEARCH_PLACEHOLDER = 'Search docs';
export const SEARCH_LABEL = 'Search docs';
export const SEARCH_HINT = '/';

export function noResultsMessage(query: string): string {
  return `No results for "${query}".`;
}

export type FocusTarget = {
  tagName?: string;
  isContentEditable?: boolean;
};

/** `/` focuses search unless a modifier is held or the user is typing in a field. */
export function slashShouldFocus(event: {
  key: string;
  metaKey?: boolean;
  ctrlKey?: boolean;
  altKey?: boolean;
  target: FocusTarget | null;
}): boolean {
  if (event.key !== '/' || event.metaKey || event.ctrlKey || event.altKey) return false;
  const target = event.target;
  if (!target) return true;
  if (target.isContentEditable) return false;
  const tag = target.tagName?.toUpperCase();
  return tag !== 'INPUT' && tag !== 'TEXTAREA' && tag !== 'SELECT';
}

export function moveResultIndex(current: number, count: number, direction: 1 | -1): number {
  if (count <= 0) return -1;
  if (current < 0 || current >= count) return direction === 1 ? 0 : count - 1;
  return (current + direction + count) % count;
}

function siteBase(baseUrl: string): string {
  if (!baseUrl || baseUrl === '/') return '';
  let base = baseUrl.trim();
  if (!base.startsWith('/')) base = `/${base}`;
  if (base.length > 1 && base.endsWith('/')) base = base.slice(0, -1);
  return base;
}

/** Pagefind records `file` build URLs (`/docs/install.html`). Clean them and keep the site base. */
export function resultHref(url: string, baseUrl: string): string {
  const hashAt = url.indexOf('#');
  const hash = hashAt >= 0 ? url.slice(hashAt) : '';
  let path = hashAt >= 0 ? url.slice(0, hashAt) : url;
  path = path.replace(/\/index\.html$/i, '/').replace(/\.html$/i, '');
  if (!path.startsWith('/')) path = `/${path}`;
  if (path.length > 1 && path.endsWith('/')) path = path.slice(0, -1);

  const base = siteBase(baseUrl);
  if (base && path !== base && !path.startsWith(`${base}/`)) path = `${base}${path}`;
  return `${path}${hash}`;
}
