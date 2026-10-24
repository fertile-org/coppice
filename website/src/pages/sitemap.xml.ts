import type { APIRoute } from 'astro';

import { englishRoutesFromPages } from '../i18n/catalog';
import { sitemapXml } from '../i18n/paths';
import { catalog } from '../i18n/translations';
import { lastCommitDate, pageSources } from '../lib/lastmod';

const pages = import.meta.glob('./**/*.{astro,md,mdx}', { eager: false });

export const GET: APIRoute = ({ site }) => {
  const xml = sitemapXml({
    englishRoutes: englishRoutesFromPages(Object.keys(pages)),
    catalog,
    base: import.meta.env.BASE_URL,
    site: site?.href,
    lastmod: (locale, englishPath) => lastCommitDate(pageSources(locale, englishPath)),
  });
  return new Response(xml, {
    headers: { 'Content-Type': 'application/xml; charset=utf-8' },
  });
};
