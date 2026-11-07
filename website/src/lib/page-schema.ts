import { AGENT_CLIS } from '../content/agents';
import { localizedPath } from '../i18n/paths';
import type { PageI18n } from '../i18n/runtime';
import { heroScreenshot, ogCard } from './hero-assets';
import type { BlogPost } from './blog';
import { blog, blogPosting, breadcrumbList, softwareApplication, techArticle, type SiteRef, type Thing } from './structured-data';
import { lastCommitDate, pageSources } from './lastmod';
import { withBase } from './url';

/** Page-level schema.org items. `Base.astro` adds Organization and WebSite. */

type AstroLike = { site?: URL | undefined };

function siteRef(astro: AstroLike): SiteRef | null {
  return astro.site ? { site: astro.site.href, base: import.meta.env.BASE_URL } : null;
}

function absolute(astro: AstroLike, path: string): string {
  return new URL(withBase(path), astro.site).href;
}

/** Language of the main content: a locale URL still showing English is English. */
function contentLocale(ctx: PageI18n): { locale: string; code: string } {
  return ctx.translated ? { locale: ctx.locale, code: ctx.code } : { locale: 'en', code: 'en' };
}

/** Homepage and download page. */
export function softwareSchema(astro: AstroLike, ctx: PageI18n, description: string): Thing[] {
  const ref = siteRef(astro);
  if (!ref) return [];
  return [
    softwareApplication(ref, {
      description,
      inLanguage: contentLocale(ctx).code,
      image: absolute(astro, ogCard),
      screenshot: absolute(astro, heroScreenshot),
    }),
  ];
}

function blogCrumbs(locale: string, ctx: PageI18n, title?: string) {
  const crumbs = [
    { name: 'Coppice', pathname: localizedPath(locale, '/') },
    { name: 'Blog', pathname: localizedPath(locale, '/blog') },
  ];
  if (title && ctx.englishPath !== '/blog') crumbs.push({ name: title, pathname: localizedPath(locale, ctx.englishPath) });
  return crumbs;
}

/** Blog index. `blogPost` is omitted when nothing is published. */
export function blogIndexSchema(
  astro: AstroLike,
  ctx: PageI18n,
  description: string,
  posts: readonly { title: string; englishPath: string }[],
): Thing[] {
  const ref = siteRef(astro);
  if (!ref) return [];
  const { locale, code } = contentLocale(ctx);
  return [
    blog(ref, {
      name: 'Coppice blog',
      description,
      pathname: localizedPath(locale, '/blog'),
      inLanguage: code,
      posts: posts.map((post) => ({ headline: post.title, pathname: localizedPath(locale, post.englishPath) })),
    }),
    breadcrumbList(ref, blogCrumbs(locale, ctx)),
  ];
}

/** One blog article. `dateModified` is `updated`, or `published` when the note was not updated. */
export function blogPostSchema(astro: AstroLike, ctx: PageI18n, post: BlogPost): Thing[] {
  const ref = siteRef(astro);
  if (!ref) return [];
  const { locale, code } = contentLocale(ctx);
  const pathname = localizedPath(locale, ctx.englishPath);
  const imagePath = post.image?.startsWith('/') ? post.image : ogCard;
  return [
    blogPosting(ref, {
      headline: post.title,
      description: post.description,
      pathname,
      inLanguage: code,
      datePublished: post.published,
      dateModified: post.updated ?? post.published,
      authorName: post.author,
      authorType: post.authorType,
      image: post.image?.startsWith('https://') ? post.image : absolute(astro, imagePath),
      keywords: post.keywords,
    }),
    breadcrumbList(ref, blogCrumbs(locale, ctx, post.title)),
  ];
}

/** Docs pages: the article and its breadcrumb trail (Coppice › Docs › page). */
export function docsSchema(astro: AstroLike, ctx: PageI18n, title: string, description: string): Thing[] {
  const ref = siteRef(astro);
  if (!ref) return [];
  const { locale, code } = contentLocale(ctx);
  const pathname = localizedPath(locale, ctx.englishPath);
  const crumbs = [
    { name: 'Coppice', pathname: localizedPath(locale, '/') },
    { name: ctx.ui.nav.docs, pathname: localizedPath(locale, '/docs') },
  ];
  if (ctx.englishPath !== '/docs') crumbs.push({ name: title, pathname });
  return [
    techArticle(ref, {
      headline: title,
      description,
      pathname,
      inLanguage: code,
      image: absolute(astro, ogCard),
      dateModified: lastCommitDate(pageSources(locale, ctx.englishPath)),
      keywords: ctx.englishPath === '/docs/providers' ? AGENT_CLIS : undefined,
    }),
    breadcrumbList(ref, crumbs),
  ];
}
