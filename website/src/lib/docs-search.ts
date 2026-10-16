/** Content-locked docs search strings. Placeholder and accessible label match. */
export const SEARCH_PLACEHOLDER = 'Search docs';
export const SEARCH_LABEL = 'Search docs';
export const SEARCH_HINT = '/';
export const SEARCH_FOOTER = '↑↓ to move, Enter to open, Esc to close';
export const SEARCH_CLOSE = 'Close';

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

export type IndexedSection = {
  url?: string;
  title?: string;
  excerpt?: string;
};

export type IndexedHit = {
  url: string;
  excerpt?: string;
  meta?: { title?: string };
  sub_results?: IndexedSection[];
};

export type SearchRow = {
  url: string;
  pageTitle: string;
  section: string;
  excerpt: string;
};

/** Flatten Pagefind page hits into rows with a page title, optional section, and snippet. */
export function searchRows(hits: IndexedHit[], limit: number): SearchRow[] {
  const rows: SearchRow[] = [];
  for (const hit of hits) {
    if (rows.length >= limit) break;
    const sections = hit.sub_results?.length
      ? hit.sub_results
      : [{ url: hit.url, title: '', excerpt: hit.excerpt }];
    for (const sectionHit of sections) {
      if (rows.length >= limit) break;
      const sectionTitle = sectionHit.title?.trim() || '';
      const url = (sectionHit.url || hit.url || '').trim();
      const pageTitle = hit.meta?.title?.trim() || sectionTitle || url;
      rows.push({
        url,
        pageTitle,
        section: sectionTitle && sectionTitle !== pageTitle ? sectionTitle : '',
        excerpt: sectionHit.excerpt ?? hit.excerpt ?? '',
      });
    }
  }
  return rows;
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
