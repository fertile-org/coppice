import { unified } from '@astrojs/markdown-remark';
import pagefind from 'astro-pagefind';
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
function prefixAbsolute(root, value) {
  if (typeof value === 'string' && value.startsWith('/') && !value.startsWith('//')) {
    return `${root}${value}`;
  }
  return value;
}

function prefixRootLinks() {
  const prefix = siteBase();
  const root = prefix === '/' ? '' : prefix;
  return (tree) => {
    if (!root) return;
    const walk = (node) => {
      if (!node || typeof node !== 'object') return;
      if (node.type === 'element' && node.properties) {
        for (const attr of ['href', 'src']) {
          node.properties[attr] = prefixAbsolute(root, node.properties[attr]);
        }
      }
      // Markdown HTML blocks stay raw, so figure <img src="/assets/..."> would
      // otherwise miss the GitHub Pages base.
      if (node.type === 'raw' && typeof node.value === 'string') {
        node.value = node.value.replace(
          /(\s(?:href|src)\s*=\s*)(["'])(\/(?!\/)[^"']*)\2/gi,
          (_match, attr, quote, url) => `${attr}${quote}${root}${url}${quote}`,
        );
      }
      if (Array.isArray(node.children)) node.children.forEach(walk);
    };
    walk(tree);
  };
}

const site = process.env.SITE_URL?.trim() || undefined;

/** Pagefind logs indexing failures and returns. Fail the build instead. */
function pagefindOrFail() {
  const integration = pagefind();
  const done = integration.hooks['astro:build:done'];
  integration.hooks['astro:build:done'] = async (options) => {
    const errors = [];
    const logger = new Proxy(options.logger, {
      get(target, prop, receiver) {
        if (prop === 'error') {
          return (message) => {
            errors.push(String(message));
            return target.error(message);
          };
        }
        const value = Reflect.get(target, prop, receiver);
        return typeof value === 'function' ? value.bind(target) : value;
      },
    });
    await done({ ...options, logger });
    if (errors.length) throw new Error(errors.join('\n'));
  };
  return integration;
}

export default defineConfig({
  site,
  base: siteBase(),
  trailingSlash: 'never',
  integrations: [pagefindOrFail()],
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
