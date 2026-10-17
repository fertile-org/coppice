import type { APIRoute } from 'astro';

import { englishRoutesFromPages } from '../i18n/catalog';
import { sitemapXml } from '../i18n/paths';
import { catalog } from '../i18n/translations';

const pages = import.meta.glob('./**/*.{astro,md,mdx}', { eager: false });

export const GET: APIRoute = ({ site }) => {
  const xml = sitemapXml({
    englishRoutes: englishRoutesFromPages(Object.keys(pages)),
    catalog,
    base: import.meta.env.BASE_URL,
    site: site?.href,
  });
  return new Response(xml, {
    headers: { 'Content-Type': 'application/xml; charset=utf-8' },
  });
};
