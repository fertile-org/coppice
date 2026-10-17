import { buildCatalog } from './catalog';
import type { LocalePath } from './paths';

type DocModule = {
  Content: (props: Record<string, unknown>) => unknown;
  frontmatter?: { title?: string; description?: string };
};

const docModules = import.meta.glob('../content/docs/*/**/*.{md,mdx}', { eager: true }) as Record<string, DocModule>;
const localePageKeys = Object.keys(import.meta.glob('../pages/*/**/*.{astro,md,mdx}', { eager: false }));
const pageStringKeys = Object.keys(import.meta.glob('./pages/*.ts', { eager: false })).map(
  (file) => `src/i18n/pages/${file.split('/').pop()}`,
);

export const catalog = buildCatalog([...Object.keys(docModules), ...localePageKeys, ...pageStringKeys]);

function findModule<T>(modules: Record<string, T>, suffix: string): T | null {
  const key = Object.keys(modules).find((item) => item.replace(/\\/g, '/').endsWith(suffix));
  return key ? modules[key] : null;
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
