import type { APIRoute } from 'astro';

import { publicHref } from '../i18n/paths';

/**
 * Every crawler is welcome, AI search crawlers included. A named group would
 * replace the `*` rules for that bot, so there is only one group.
 * `/pagefind/` is the docs search index, not pages.
 */
export const GET: APIRoute = ({ site }) => {
  const base = import.meta.env.BASE_URL;
  const body = [
    'User-agent: *',
    'Allow: /',
    `Disallow: ${publicHref('/pagefind/', base)}`,
    '',
    `Sitemap: ${publicHref('/sitemap.xml', base, site?.href)}`,
    '',
  ].join('\n');
  return new Response(body, {
    headers: { 'Content-Type': 'text/plain; charset=utf-8' },
  });
};
