import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import { buildCatalog, englishRoutesFromPages, matchTranslation } from './catalog.ts';
import { prefixHref, rewriteLocaleHtml } from './locale-html.mjs';
import { LOCALES } from './locales.mjs';
import {
  alternateLinks,
  canonicalHref,
  localeFromPathname,
  localizedPath,
  pagefindHasLanguage,
  sitemapXml,
  translatedLocales,
  urlInLocale,
} from './paths.ts';

describe('locale registry', () => {
  it('uses lowercase URL prefixes and the requested language codes', () => {
    assert.deepEqual(
      LOCALES.map((locale) => [locale.path, locale.code, locale.name]),
      [
        ['en', 'en', 'English'],
        ['zh-cn', 'zh-CN', '简体中文'],
        ['zh-tw', 'zh-TW', '繁體中文（台灣）'],
        ['zh-hk', 'zh-HK', '繁體中文（香港）'],
        ['ja', 'ja', '日本語'],
        ['fr', 'fr', 'Français'],
      ],
    );
  });
});

describe('locale paths', () => {
  it('keeps English URLs unprefixed, including .html build paths', () => {
    assert.deepEqual(localeFromPathname('/docs/concepts.html'), { locale: 'en', englishPath: '/docs/concepts' });
    assert.deepEqual(localeFromPathname('/index.html'), { locale: 'en', englishPath: '/' });
    assert.equal(localizedPath('en', '/docs/concepts'), '/docs/concepts');
    assert.equal(localizedPath('en', '/'), '/');
  });

  it('reads lowercase locale prefixes and maps them back', () => {
    assert.deepEqual(localeFromPathname('/ja/docs/concepts.html'), {
      locale: 'ja',
      englishPath: '/docs/concepts',
    });
    assert.deepEqual(localeFromPathname('/zh-cn.html'), { locale: 'zh-cn', englishPath: '/' });
    assert.deepEqual(localeFromPathname('/coppice/zh-tw/download.html', '/coppice'), {
      locale: 'zh-tw',
      englishPath: '/download',
    });
    assert.equal(localizedPath('zh-hk', '/docs/concepts'), '/zh-hk/docs/concepts');
    assert.equal(localizedPath('fr', '/'), '/fr');
  });
});

describe('hreflang and canonical', () => {
  it('lists only existing translations plus x-default on a fallback page', () => {
    const catalog = buildCatalog([]);
    const locales = translatedLocales(catalog, '/docs/concepts');
    assert.deepEqual(locales, ['en']);
    assert.deepEqual(alternateLinks('/docs/concepts', locales), [
      { hreflang: 'en', href: '/docs/concepts' },
      { hreflang: 'x-default', href: '/docs/concepts' },
    ]);
    assert.equal(canonicalHref('ja', '/docs/concepts', false), '/docs/concepts');
  });

  it('points a real translation at itself and adds that locale to the alternates', () => {
    const catalog = buildCatalog(['src/content/docs/ja/concepts.md']);
    const locales = translatedLocales(catalog, '/docs/concepts');
    assert.deepEqual(locales, ['en', 'ja']);
    const links = alternateLinks('/docs/concepts', locales, '/', 'https://getcoppice.vercel.app');
    assert.deepEqual(links, [
      { hreflang: 'en', href: 'https://getcoppice.vercel.app/docs/concepts' },
      { hreflang: 'ja', href: 'https://getcoppice.vercel.app/ja/docs/concepts' },
      { hreflang: 'x-default', href: 'https://getcoppice.vercel.app/docs/concepts' },
    ]);
    assert.equal(
      canonicalHref('ja', '/docs/concepts', true, '/', 'https://getcoppice.vercel.app'),
      'https://getcoppice.vercel.app/ja/docs/concepts',
    );
    assert.equal(canonicalHref('zh-cn', '/docs/install', false, '/coppice', 'https://fertile-org.github.io'), 'https://fertile-org.github.io/coppice/docs/install');
  });

  it('omits an untranslated locale from the sitemap and includes alternates for a real one', () => {
    const xml = sitemapXml({
      englishRoutes: ['/docs/concepts', '/docs/install'],
      catalog: buildCatalog(['src/pages/ja/docs/concepts.md']),
      site: 'https://getcoppice.vercel.app',
    });
    assert.match(xml, /<loc>https:\/\/getcoppice\.vercel\.app\/docs\/concepts<\/loc>/);
    assert.match(xml, /<loc>https:\/\/getcoppice\.vercel\.app\/ja\/docs\/concepts<\/loc>/);
    assert.match(xml, /hreflang="ja" href="https:\/\/getcoppice\.vercel\.app\/ja\/docs\/concepts"/);
    assert.match(xml, /hreflang="x-default" href="https:\/\/getcoppice\.vercel\.app\/docs\/concepts"/);
    assert.doesNotMatch(xml, /\/ja\/docs\/install/);
    assert.doesNotMatch(xml, /hreflang="zh-CN"/);
  });
});

describe('translation sources', () => {
  it('accepts content files and localized pages, and ignores homepage shells', () => {
    assert.deepEqual(matchTranslation('../content/docs/zh-cn/concepts.md'), {
      locale: 'zh-cn',
      englishPath: '/docs/concepts',
    });
    assert.deepEqual(matchTranslation('src/content/docs/fr/index.md'), { locale: 'fr', englishPath: '/docs' });
    assert.deepEqual(matchTranslation('src/content/pages/ja/index.astro'), { locale: 'ja', englishPath: '/' });
    assert.deepEqual(matchTranslation('src/pages/zh-tw/docs/install.md'), {
      locale: 'zh-tw',
      englishPath: '/docs/install',
    });
    assert.deepEqual(matchTranslation('src/pages/zh-hk/download.astro'), {
      locale: 'zh-hk',
      englishPath: '/download',
    });
    assert.equal(matchTranslation('src/pages/ja/index.astro'), null);
    assert.equal(matchTranslation('src/pages/docs/concepts.md'), null);
  });

  it('lists English routes from page files and skips locale shells', () => {
    assert.deepEqual(
      englishRoutesFromPages([
        './index.astro',
        './download.astro',
        './404.astro',
        './sitemap.xml.ts',
        './docs/index.md',
        './docs/concepts.md',
        './ja/index.astro',
        './ja/docs/concepts.md',
      ]),
      ['/', '/docs', '/docs/concepts', '/download'],
    );
  });
});

describe('locale anchors', () => {
  it('prefixes site links and leaves assets, canonicals, and existing locale links', () => {
    assert.equal(prefixHref('/docs/concepts#chat', 'zh-cn', '/'), '/zh-cn/docs/concepts#chat');
    assert.equal(prefixHref('/coppice/docs/install', 'ja', '/coppice'), '/coppice/ja/docs/install');
    assert.equal(prefixHref('/', 'fr', '/'), '/fr');
    assert.equal(prefixHref('/ja/docs/faq', 'ja', '/'), '/ja/docs/faq');
    assert.equal(prefixHref('/logo.svg', 'ja', '/'), '/logo.svg');
    assert.equal(prefixHref('https://github.com/fertile-org/coppice', 'ja', '/'), 'https://github.com/fertile-org/coppice');

    const html = rewriteLocaleHtml(
      '<a href="/docs/install">Install</a><link rel="canonical" href="/docs/install"><a href="/ja/docs/faq">FAQ</a>',
      'ja',
      '/',
    );
    assert.equal(
      html,
      '<a href="/ja/docs/install">Install</a><link rel="canonical" href="/docs/install"><a href="/ja/docs/faq">FAQ</a>',
    );
  });
});

describe('search language', () => {
  it('searches a locale only when that language index exists', () => {
    assert.equal(pagefindHasLanguage({ en: { page_count: 10 } }, 'ja'), false);
    assert.equal(pagefindHasLanguage({ en: {}, 'zh-cn': {} }, 'zh-CN'), true);
    assert.equal(urlInLocale('/docs/install.html', 'en'), true);
    assert.equal(urlInLocale('/ja/docs/install.html', 'en'), false);
    assert.equal(urlInLocale('/ja/docs/concepts.html', 'ja'), true);
    assert.equal(urlInLocale('/coppice/zh-cn/docs/concepts.html', 'zh-cn', '/coppice'), true);
  });
});
