import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { unified } from '@astrojs/markdown-remark';
import pagefind from 'astro-pagefind';
import { defineConfig } from 'astro/config';
import { localeFromDistFile, rewriteLocaleHtml } from './src/i18n/locale-html.mjs';
import { astroLocales, DEFAULT_LOCALE, fallbackMap, LOCALE_PATHS } from './src/i18n/locales.mjs';

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

/** Prefix in-page links on locale HTML before Pagefind reads the output. */
function localeLinkRewrite() {
  return {
    name: 'locale-link-rewrite',
    hooks: {
      'astro:build:done': ({ dir }) => {
        const root = fileURLToPath(dir);
        const walk = (directory) => {
          for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
            const full = path.join(directory, entry.name);
            if (entry.isDirectory()) {
              if (entry.name === 'pagefind' || entry.name === '_astro') continue;
              walk(full);
              continue;
            }
            if (!entry.name.endsWith('.html')) continue;
            const locale = localeFromDistFile(path.relative(root, full));
            if (!locale) continue;
            const html = fs.readFileSync(full, 'utf8');
            const next = rewriteLocaleHtml(html, locale);
            if (next !== html) fs.writeFileSync(full, next);
          }
        };
        walk(root);
      },
    },
  };
}

function chatRedirects() {
  const redirects = {
    '/docs/chat': '/docs/concepts#chat',
  };
  for (const locale of LOCALE_PATHS) {
    redirects[`/${locale}/docs/chat`] = `/${locale}/docs/concepts#chat`;
  }
  return redirects;
}

export default defineConfig({
  site: 'https://getcoppice.dev',
  trailingSlash: 'never',
  i18n: {
    defaultLocale: DEFAULT_LOCALE,
    locales: astroLocales(),
    fallback: fallbackMap(),
    routing: {
      prefixDefaultLocale: false,
      redirectToDefaultLocale: false,
      fallbackType: 'rewrite',
    },
  },
  integrations: [localeLinkRewrite(), pagefindOrFail()],
  // `file` writes docs.html. Vercel serves that at /docs via website/vercel.json
  // (`cleanUrls` + `trailingSlash: false`).
  build: { format: 'file' },
  redirects: chatRedirects(),
  markdown: {
    processor: unified({
      gfm: true,
      smartypants: true,
    }),
  },
});
