import { unified } from '@astrojs/markdown-remark';
import { defineConfig } from 'astro/config';

/** `/` or a path prefix such as `/coppice`, with no trailing slash. */
function normalizeBase(value) {
  if (!value || value === '/') return '/';
  let base = String(value).trim();
  if (!base.startsWith('/')) base = `/${base}`;
  if (base.length > 1 && base.endsWith('/')) base = base.slice(0, -1);
  return base;
}

/**
 * Domain root by default (Vercel). `BASE_PATH` wins; `SITE_BASE` is the older name.
 * GitHub project Pages sets this to `/coppice`.
 */
function siteBase() {
  return normalizeBase(process.env.BASE_PATH ?? process.env.SITE_BASE);
}

/**
 * Markdown links in the drafts are root-relative (`/docs/...`).
 * Prefix them when the site is hosted below a base path (GitHub project Pages).
 */
function prefixRootLinks() {
  const prefix = siteBase();
  const root = prefix === '/' ? '' : prefix;
  return (tree) => {
    if (!root) return;
    const walk = (node) => {
      if (!node || typeof node !== 'object') return;
      if (node.type === 'element' && node.properties) {
        for (const attr of ['href', 'src']) {
          const value = node.properties[attr];
          if (typeof value === 'string' && value.startsWith('/') && !value.startsWith('//')) {
            node.properties[attr] = `${root}${value}`;
          }
        }
      }
      if (Array.isArray(node.children)) node.children.forEach(walk);
    };
    walk(tree);
  };
}

const site = process.env.SITE_URL?.trim() || undefined;

export default defineConfig({
  site,
  base: siteBase(),
  trailingSlash: 'never',
  // `file` writes docs.html. Vercel serves that at /docs via website/vercel.json
  // (`cleanUrls` + `trailingSlash: false`). GitHub Pages has no clean URLs, so the
  // Pages deploy rewrites those files to docs/index.html after the build.
  build: { format: 'file' },
  markdown: {
    processor: unified({
      gfm: true,
      smartypants: true,
      rehypePlugins: [prefixRootLinks],
    }),
  },
});
