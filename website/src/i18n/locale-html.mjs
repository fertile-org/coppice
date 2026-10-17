import { LOCALE_PATHS } from './locales.mjs';

const LOCALE_SET = new Set(LOCALE_PATHS);

function normalizeBase(base) {
  if (!base || base === '/') return '';
  let root = String(base).trim();
  if (!root.startsWith('/')) root = `/${root}`;
  if (root.length > 1 && root.endsWith('/')) root = root.slice(0, -1);
  return root;
}

/** First URL segment of a built HTML file, when that file is a non-default locale page. */
export function localeFromDistFile(relativePath) {
  const normalized = relativePath.split('\\').join('/');
  const withoutExt = normalized.replace(/\.html$/, '');
  if (LOCALE_SET.has(withoutExt)) return withoutExt;
  const first = normalized.split('/')[0];
  return LOCALE_SET.has(first) ? first : null;
}

function isSitePath(pathname) {
  return pathname === '/' || pathname === '/download' || pathname === '/docs' || pathname.startsWith('/docs/');
}

/**
 * Prefix an in-page link so a locale URL stays in that locale.
 * Canonical and hreflang tags are not anchors, so callers should only pass `<a href>`.
 */
export function prefixHref(href, locale, base = '/') {
  if (!href || !LOCALE_SET.has(locale)) return href;
  if (href.startsWith('#') || href.startsWith('//')) return href;
  if (/^[a-z][a-z0-9+.-]*:/i.test(href)) return href;
  if (!href.startsWith('/')) return href;

  const hashAt = href.indexOf('#');
  const hash = hashAt >= 0 ? href.slice(hashAt) : '';
  let path = hashAt >= 0 ? href.slice(0, hashAt) : href;
  const queryAt = path.indexOf('?');
  const query = queryAt >= 0 ? path.slice(queryAt) : '';
  if (queryAt >= 0) path = path.slice(0, queryAt);

  const root = normalizeBase(base);
  let rest = path;
  if (root && (rest === root || rest.startsWith(`${root}/`))) rest = rest.slice(root.length) || '/';
  if (!rest.startsWith('/')) rest = `/${rest}`;
  if (rest.length > 1 && rest.endsWith('/')) rest = rest.slice(0, -1);

  const first = rest.split('/')[1];
  if (LOCALE_SET.has(first)) return href;
  if (!isSitePath(rest)) return href;

  const localized = rest === '/' ? `/${locale}` : `/${locale}${rest}`;
  return `${root}${localized}${query}${hash}`;
}

/** Rewrite root-relative anchors in a locale page. Leaves `<link>` tags alone. */
export function rewriteLocaleHtml(html, locale, base = '/') {
  if (!LOCALE_SET.has(locale)) return html;
  return html.replace(/\bhref\s*=\s*(["'])([^"']*)\1/gi, (match, quote, href, offset, source) => {
    const start = source.lastIndexOf('<', offset);
    if (start < 0) return match;
    const tag = source.slice(start + 1, offset).trimStart().toLowerCase();
    if (!tag.startsWith('a') || (tag.length > 1 && !/[\s/>]/.test(tag[1]))) return match;
    const next = prefixHref(href, locale, base);
    if (next === href) return match;
    return `href=${quote}${next}${quote}`;
  });
}
