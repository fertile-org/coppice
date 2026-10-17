import { buildCatalog } from './catalog';
import type { LocalePath } from './paths';

type DocModule = {
  Content: (props: Record<string, unknown>) => unknown;
  frontmatter?: { title?: string; description?: string };
};

type PageModule = {
  default?: (props: Record<string, unknown>) => unknown;
  Content?: (props: Record<string, unknown>) => unknown;
};

const docModules = import.meta.glob('../content/docs/*/**/*.{md,mdx}', { eager: true }) as Record<string, DocModule>;
const pageModules = import.meta.glob('../content/pages/*/**/*.{astro,md,mdx}', { eager: true }) as Record<
  string,
  PageModule
>;
const localePageKeys = Object.keys(import.meta.glob('../pages/*/**/*.{astro,md,mdx}', { eager: false }));

export const catalog = buildCatalog([
  ...Object.keys(docModules),
  ...Object.keys(pageModules),
  ...localePageKeys,
]);

function findModule<T>(modules: Record<string, T>, suffix: string): T | null {
  const key = Object.keys(modules).find((item) => item.replace(/\\/g, '/').endsWith(suffix));
  return key ? modules[key] : null;
}

/** Whole-page component for `/` or `/download`, when a locale added one under `src/content/pages`. */
export function pageOverride(locale: string, englishPath: '/' | '/download') {
  if (locale === 'en') return null;
  const name = englishPath === '/' ? 'index' : 'download';
  for (const ext of ['.astro', '.md', '.mdx']) {
    const mod = findModule(pageModules, `/content/pages/${locale}/${name}${ext}`);
    const component = mod?.default ?? mod?.Content;
    if (component) return component;
  }
  return null;
}

/** Docs markdown that replaces the English article on a fallback URL. */
export function docOverride(
  locale: string,
  englishPath: string,
): { Content: DocModule['Content']; title?: string; description?: string } | null {
  if (locale === 'en' || !englishPath.startsWith('/docs')) return null;
  const slug = englishPath === '/docs' ? 'index' : englishPath.slice('/docs/'.length);
  for (const ext of ['.md', '.mdx']) {
    const mod = findModule(docModules, `/content/docs/${locale}/${slug}${ext}`);
    if (mod?.Content) {
      return { Content: mod.Content, title: mod.frontmatter?.title, description: mod.frontmatter?.description };
    }
  }
  return null;
}

export function localesFor(englishPath: string): Set<LocalePath> | undefined {
  return catalog.get(englishPath);
}
