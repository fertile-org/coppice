import { DEFAULT_LOCALE, LOCALES } from './locales.mjs';

export { DEFAULT_LOCALE, LOCALES };

export type LocalePath = 'en' | 'vi' | 'zh-cn' | 'zh-tw' | 'zh-hk' | 'ja' | 'fr';

const LOCALE_BY_PATH = new Map(LOCALES.map((locale) => [locale.path, locale]));
const NON_DEFAULT = LOCALES.filter((locale) => locale.path !== DEFAULT_LOCALE).map((locale) => locale.path);

export function isLocalePath(value: string): value is LocalePath {
  return LOCALE_BY_PATH.has(value);
}

export function localeCode(locale: string): string {
  return LOCALE_BY_PATH.get(locale)?.code ?? locale;
}

/** Open Graph locale (`ja_JP`) for a URL prefix or a BCP 47 code. */
export function ogLocale(locale: string): string {
  const found = LOCALE_BY_PATH.get(locale) ?? LOCALES.find((item) => item.code === locale);
  return found?.og ?? 'en_US';
}

export function normalizeBase(base: string | undefined): string {
  if (!base || base === '/') return '';
  let root = String(base).trim();
  if (!root.startsWith('/')) root = `/${root}`;
  if (root.length > 1 && root.endsWith('/')) root = root.slice(0, -1);
  return root;
}

/** Strip the site base, `.html`, and a trailing slash. */
export function normalizePathname(pathname: string, base: string | undefined = '/'): string {
  const root = normalizeBase(base);
  let path = pathname || '/';
  if (root && (path === root || path.startsWith(`${root}/`))) path = path.slice(root.length) || '/';
  path = path.replace(/\/index\.html$/i, '/').replace(/\.html$/i, '');
  if (!path.startsWith('/')) path = `/${path}`;
  if (path.length > 1 && path.endsWith('/')) path = path.slice(0, -1);
  return path || '/';
}

export function localeFromPathname(
  pathname: string,
  base: string | undefined = '/',
): { locale: LocalePath; englishPath: string } {
  const path = normalizePathname(pathname, base);
  const segment = path.split('/')[1] ?? '';
  if (segment && segment !== DEFAULT_LOCALE && isLocalePath(segment)) {
    const rest = path.slice(segment.length + 1);
    return { locale: segment, englishPath: rest === '' ? '/' : rest };
  }
  return { locale: DEFAULT_LOCALE, englishPath: path };
}

export function localizedPath(locale: string, englishPath: string): string {
  const path = englishPath.startsWith('/') ? englishPath : `/${englishPath}`;
  if (locale === DEFAULT_LOCALE || !isLocalePath(locale)) return path === '' ? '/' : path;
  if (path === '/') return `/${locale}`;
  return `/${locale}${path}`;
}

export function joinBase(base: string | undefined, pathname: string): string {
  const root = normalizeBase(base);
  if (pathname === '/') return root || '/';
  return `${root}${pathname}`;
}

export function publicHref(pathname: string, base: string | undefined = '/', site?: string): string {
  const joined = joinBase(base, pathname);
  if (!site) return joined;
  return new URL(joined, site).href;
}

export function translatedLocales(catalog: ReadonlyMap<string, ReadonlySet<string>>, englishPath: string): LocalePath[] {
  const found = catalog.get(englishPath);
  const locales: LocalePath[] = [DEFAULT_LOCALE];
  for (const locale of NON_DEFAULT) {
    if (found?.has(locale)) locales.push(locale);
  }
  return locales;
}

export function alternateLinks(
  englishPath: string,
  locales: readonly string[],
  base: string | undefined = '/',
  site?: string,
): { hreflang: string; href: string }[] {
  const links = locales.map((locale) => ({
    hreflang: localeCode(locale),
    href: publicHref(localizedPath(locale, englishPath), base, site),
  }));
  links.push({
    hreflang: 'x-default',
    href: publicHref(localizedPath(DEFAULT_LOCALE, englishPath), base, site),
  });
  return links;
}

export function canonicalHref(
  locale: string,
  englishPath: string,
  translated: boolean,
  base: string | undefined = '/',
  site?: string,
): string {
  const path = translated ? localizedPath(locale, englishPath) : localizedPath(DEFAULT_LOCALE, englishPath);
  return publicHref(path, base, site);
}

export function pagefindHasLanguage(
  languages: Record<string, unknown> | undefined,
  lang: string,
): boolean {
  if (!languages) return false;
  const wanted = lang.toLowerCase();
  return Object.keys(languages).some((key) => key.toLowerCase() === wanted);
}

/** Pagefind result URLs belong to one locale. English rejects prefixed locale URLs. */
export function urlInLocale(url: string, locale: string, base: string | undefined = '/'): boolean {
  const { locale: found } = localeFromPathname(url, base);
  return found === locale.toLowerCase();
}

function xmlEscape(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

export function sitemapXml(options: {
  englishRoutes: readonly string[];
  catalog: ReadonlyMap<string, ReadonlySet<string>>;
  base?: string;
  site?: string;
  /** `YYYY-MM-DD` for a page, or undefined to leave out `<lastmod>`. */
  lastmod?: (locale: LocalePath, englishPath: string) => string | undefined;
}): string {
  const urls: string[] = [];
  for (const englishPath of options.englishRoutes) {
    const locales = translatedLocales(options.catalog, englishPath);
    const alternates = alternateLinks(englishPath, locales, options.base, options.site);
    const links = alternates
      .map((link) => `    <xhtml:link rel="alternate" hreflang="${xmlEscape(link.hreflang)}" href="${xmlEscape(link.href)}" />`)
      .join('\n');
    for (const locale of locales) {
      const loc = publicHref(localizedPath(locale, englishPath), options.base, options.site);
      const date = options.lastmod?.(locale, englishPath);
      const lastmod = date ? `\n    <lastmod>${xmlEscape(date)}</lastmod>` : '';
      urls.push(`  <url>\n    <loc>${xmlEscape(loc)}</loc>${lastmod}\n${links}\n  </url>`);
    }
  }
  return `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">\n${urls.join('\n')}\n</urlset>\n`;
}
