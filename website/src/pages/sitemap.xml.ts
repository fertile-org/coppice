import type { APIRoute } from 'astro';

import { englishRoutesFromPages } from '../i18n/catalog';
import { sitemapXml } from '../i18n/paths';
import { catalog } from '../i18n/translations';
import { blogLastmod, englishArticlePaths } from '../lib/blog';
import { blogEntries } from '../lib/blog-runtime';
import { lastCommitDate, pageSources } from '../lib/lastmod';

const pages = import.meta.glob('./**/*.{astro,md,mdx}', { eager: false });

export const GET: APIRoute = ({ site }) => {
  const posts = blogEntries().map((entry) => entry.post);
  const englishRoutes = [...englishRoutesFromPages(Object.keys(pages)), ...englishArticlePaths(posts)].sort((a, b) =>
    a.localeCompare(b),
  );
  const xml = sitemapXml({
    englishRoutes,
    catalog,
    base: import.meta.env.BASE_URL,
    site: site?.href,
    lastmod: (locale, englishPath) =>
      englishPath === '/blog' || englishPath.startsWith('/blog/')
        ? blogLastmod(posts, locale, englishPath)
        : lastCommitDate(pageSources(locale, englishPath)),
  });
  return new Response(xml, {
    headers: { 'Content-Type': 'application/xml; charset=utf-8' },
  });
};
