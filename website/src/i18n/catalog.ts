import { LOCALE_PATHS } from './locales.mjs';
import { isLocalePath, type LocalePath } from './paths.ts';

const LOCALE_SET = new Set<string>(LOCALE_PATHS);

/** Turn a glob key (`../pages/ja/docs/concepts.md` or `src/pages/...`) into a repo path under `src/`. */
export function normalizeSourcePath(file: string): string {
  let path = file.replace(/\\/g, '/');
  if (path.startsWith('./')) path = path.slice(2);
  if (path.startsWith('../')) path = `src/${path.slice(3)}`;
  return path;
}

function docsEnglishPath(relative: string): string | null {
  const noExt = relative.replace(/\.(md|mdx|astro)$/i, '');
  if (!noExt || noExt.endsWith('.')) return null;
  if (noExt === 'index') return '/docs';
  if (noExt.endsWith('/index')) return `/docs/${noExt.slice(0, -'/index'.length)}`;
  return `/docs/${noExt}`;
}

/**
 * A real translation source for a non-default locale.
 * Homepage shells at `src/pages/<locale>/index.astro` are infrastructure, not translations.
 */
export function matchTranslation(file: string): { locale: LocalePath; englishPath: string } | null {
  const path = normalizeSourcePath(file);
  let match = path.match(/^src\/content\/docs\/([^/]+)\/(.+)$/);
  if (match && LOCALE_SET.has(match[1]) && isLocalePath(match[1])) {
    const englishPath = docsEnglishPath(match[2]);
    return englishPath ? { locale: match[1], englishPath } : null;
  }
  match = path.match(/^src\/pages\/([^/]+)\/docs\/(.+)$/);
  if (match && LOCALE_SET.has(match[1]) && isLocalePath(match[1])) {
    const englishPath = docsEnglishPath(match[2]);
    return englishPath ? { locale: match[1], englishPath } : null;
  }
  return null;
}

/** `src/i18n/pages/<locale>.ts` supplies homepage and download wording. */
export function pageStringLocale(file: string): LocalePath | null {
  const path = normalizeSourcePath(file);
  const match = path.match(/(?:^|\/)i18n\/pages\/([^/]+)\.ts$/) ?? path.match(/^pages\/([^/]+)\.ts$/);
  if (!match || !LOCALE_SET.has(match[1]) || !isLocalePath(match[1])) return null;
  return match[1];
}

export function buildCatalog(files: readonly string[]): Map<string, Set<LocalePath>> {
  const catalog = new Map<string, Set<LocalePath>>();
  const add = (englishPath: string, locale: LocalePath) => {
    const locales = catalog.get(englishPath) ?? new Set<LocalePath>();
    locales.add(locale);
    catalog.set(englishPath, locales);
  };
  for (const file of files) {
    const pageLocale = pageStringLocale(file);
    if (pageLocale) {
      add('/', pageLocale);
      add('/download', pageLocale);
      continue;
    }
    const hit = matchTranslation(file);
    if (!hit) continue;
    add(hit.englishPath, hit.locale);
  }
  return catalog;
}

/** English page routes from `src/pages` keys. Locale shells, 404, and the sitemap are not included. */
export function englishRoutesFromPages(files: readonly string[]): string[] {
  const routes = new Set<string>();
  for (const file of files) {
    let path = normalizeSourcePath(file).replace(/^src\/pages\//, '');
    if (!path || path.includes('[')) continue;
    if (path.startsWith('404.') || path.startsWith('sitemap.')) continue;
    const first = path.split('/')[0]?.replace(/\.(astro|md|mdx)$/i, '') ?? '';
    if (LOCALE_SET.has(first)) continue;
    if (path === 'index.astro' || path === 'index.md') {
      routes.add('/');
      continue;
    }
    if (path === 'download.astro' || path === 'download.md') {
      routes.add('/download');
      continue;
    }
    if (path.startsWith('docs/')) {
      const englishPath = docsEnglishPath(path.slice('docs/'.length));
      if (englishPath) routes.add(englishPath);
    }
  }
  return [...routes].sort((a, b) => a.localeCompare(b));
}
