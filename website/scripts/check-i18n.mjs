import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import { LOCALES, LOCALE_PATHS } from '../src/i18n/locales.mjs';

const root = process.cwd();
const dist = path.resolve('dist');
const locales = LOCALE_PATHS;
const failures = [];

function fail(message) {
  failures.push(message);
}

function walk(dir, out = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === 'pagefind' || entry.name === '_astro') continue;
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full, out);
    else if (entry.name.endsWith('.html') || entry.name.endsWith('.xml')) out.push(full);
  }
  return out;
}

function routeOf(file, outDir) {
  let rel = path.relative(outDir, file).split(path.sep).join('/');
  if (rel.endsWith('/index.html')) rel = rel.slice(0, -'/index.html'.length);
  else if (rel.endsWith('.html')) rel = rel.slice(0, -'.html'.length);
  else if (rel.endsWith('.xml')) rel = rel.slice(0, -'.xml'.length) === 'sitemap' ? 'sitemap.xml' : rel;
  if (rel === 'index' || rel === '') return '/';
  return `/${rel}`;
}

function splitRoute(route) {
  const segment = route.split('/')[1] ?? '';
  if (locales.includes(segment)) {
    const rest = route.slice(segment.length + 1);
    return { locale: segment, englishPath: rest || '/' };
  }
  return { locale: 'en', englishPath: route };
}

function readHtml(outDir, route) {
  const rel = route === '/' ? 'index.html' : `${route.slice(1)}.html`;
  const file = path.join(outDir, rel);
  if (!fs.existsSync(file)) return null;
  return fs.readFileSync(file, 'utf8');
}

function attr(html, relValue) {
  const tags = [...html.matchAll(/<link\b[^>]*>/gi)].map((match) => match[0]);
  return tags
    .filter((tag) => new RegExp(`\\brel=["']${relValue}["']`, 'i').test(tag))
    .map((tag) => ({
      hreflang: tag.match(/\bhreflang=["']([^"']+)["']/i)?.[1] ?? '',
      href: tag.match(/\bhref=["']([^"']+)["']/i)?.[1] ?? '',
    }));
}

function pathnameOf(href) {
  if (!href) return '';
  if (href.startsWith('/')) return href.split(/[?#]/)[0];
  return new URL(href).pathname.replace(/\/$/, '') || '/';
}

/** The origin in `astro.config.mjs` `site`. Canonical, hreflang, and sitemap read that value. */
function configuredSite() {
  const source = fs.readFileSync(path.join(root, 'astro.config.mjs'), 'utf8');
  const site = source.match(/\bsite:\s*['"](https?:\/\/[^'"]+)['"]/)?.[1];
  if (!site) throw new Error('astro.config.mjs is missing site');
  return site.replace(/\/$/, '');
}

function htmlLang(html) {
  return html.match(/<html[^>]*\blang=["']([^"']+)["']/i)?.[1] ?? '';
}

function indexedUrls(outDir) {
  const fragmentDir = path.join(outDir, 'pagefind', 'fragment');
  if (!fs.existsSync(fragmentDir)) return [];
  const urls = [];
  for (const name of fs.readdirSync(fragmentDir)) {
    if (!name.endsWith('.pf_fragment')) continue;
    const raw = zlib.gunzipSync(fs.readFileSync(path.join(fragmentDir, name)));
    const record = JSON.parse(raw.subarray(raw.indexOf('{')).toString('utf8'));
    urls.push(record.url);
  }
  return urls;
}

const LOCALE_CODES = Object.fromEntries(LOCALES.map((locale) => [locale.path, locale.code]));

/** Translations that ship in the repo (not the fixture): `[hreflang, localized path]` for one English path. */
function shippedTranslations(englishPath) {
  const out = [];
  for (const locale of locales) {
    let found = false;
    if (englishPath === '/' || englishPath === '/download') {
      found = fs.existsSync(path.join(root, 'src/i18n/pages', `${locale}.ts`));
    } else if (englishPath === '/docs' || englishPath.startsWith('/docs/')) {
      const slug = englishPath === '/docs' ? 'index' : englishPath.slice('/docs/'.length);
      found = ['.md', '.mdx'].some(
        (ext) =>
          fs.existsSync(path.join(root, 'src/content/docs', locale, `${slug}${ext}`)) ||
          fs.existsSync(path.join(root, 'src/pages', locale, 'docs', `${slug}${ext}`)),
      );
    }
    if (found) out.push([LOCALE_CODES[locale], englishPath === '/' ? `/${locale}` : `/${locale}${englishPath}`]);
  }
  return out;
}

function checkHead(html, { lang, canonicalPath, hreflangs, absent = [], site, englishPath }) {
  const shipped = englishPath ? shippedTranslations(englishPath) : [];
  hreflangs = [...hreflangs, ...shipped.filter(([code]) => !hreflangs.some(([have]) => have === code))];
  absent = absent.filter((code) => !shipped.some(([have]) => have.toLowerCase() === code.toLowerCase()));
  if (htmlLang(html) !== lang) fail(`expected html lang ${lang}, got ${htmlLang(html)}`);
  const canonical = attr(html, 'canonical');
  if (canonical.length !== 1 || pathnameOf(canonical[0].href) !== canonicalPath) {
    fail(`canonical ${canonical.map((link) => link.href).join(', ') || '(missing)'} !== ${canonicalPath}`);
  }
  if (site && canonical[0] && !canonical[0].href.startsWith(`${site}/`)) {
    fail(`canonical ${canonical[0].href} is not on ${site}`);
  }
  if (site) {
    const sitemap = attr(html, 'sitemap');
    if (sitemap.length !== 1 || !sitemap[0].href.startsWith(`${site}/`)) {
      fail(`sitemap link ${sitemap.map((link) => link.href).join(', ') || '(missing)'} is not on ${site}`);
    }
  }
  const alternates = attr(html, 'alternate').filter((link) => link.hreflang);
  if (site) {
    for (const link of alternates) {
      if (!link.href.startsWith(`${site}/`) && link.href !== `${site}/`) fail(`hreflang ${link.hreflang} ${link.href} is not on ${site}`);
    }
  }
  const got = alternates.map((link) => `${link.hreflang} ${pathnameOf(link.href)}`).sort();
  const want = hreflangs.map(([code, href]) => `${code} ${href}`).sort();
  if (JSON.stringify(got) !== JSON.stringify(want)) fail(`hreflang ${got.join(' | ')} !== ${want.join(' | ')}`);
  for (const code of absent) {
    if (alternates.some((link) => link.hreflang.toLowerCase() === code.toLowerCase())) {
      fail(`hreflang includes ${code}`);
    }
  }
}

function checkFallback(outDir, locale, englishPath, site) {
  const localized = `/${locale}${englishPath}`;
  const html = readHtml(outDir, localized);
  if (!html) {
    fail(`missing ${localized}`);
    return;
  }
  // Redirect stubs (like /docs/chat) have no head to check.
  if (/http-equiv=["']refresh["']/i.test(html)) return;
  checkHead(html, {
    englishPath,
    lang: LOCALE_CODES[locale],
    canonicalPath: englishPath,
    site,
    hreflangs: [
      ['en', englishPath],
      ['x-default', englishPath],
    ],
    absent: [LOCALE_CODES[locale]],
  });
  if (html.includes('data-pagefind-body')) fail(`fallback ${localized} is in the search index`);
  if (indexedUrls(outDir).some((url) => pathnameOf(url).replace(/\.html$/, '') === localized)) {
    fail(`fallback ${localized} was indexed`);
  }
}

function checkBuild(outDir, { translated, site }) {
  const routes = walk(outDir)
    .filter((file) => file.endsWith('.html'))
    .map((file) => routeOf(file, outDir))
    .sort();
  const english = routes.filter((route) => splitRoute(route).locale === 'en');
  const baseline = fs
    .readFileSync(path.join(root, 'scripts/english-routes.txt'), 'utf8')
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean);
  const allowedExtra = new Set(['/sitemap.xml']);
  for (const route of baseline) {
    if (!english.includes(route)) fail(`missing English route ${route}`);
  }
  for (const route of english) {
    if (!baseline.includes(route) && !allowedExtra.has(route)) fail(`unexpected English route ${route}`);
  }
  for (const locale of locales) {
    for (const route of baseline) {
      const localized = route === '/' ? `/${locale}` : `/${locale}${route}`;
      if (!routes.includes(localized)) fail(`missing ${localized}`);
    }
  }

  for (const englishPath of baseline) {
    for (const [code, localized] of shippedTranslations(englishPath)) {
      const html = readHtml(outDir, localized);
      if (!html) {
        fail(`missing shipped translation ${localized}`);
        continue;
      }
      checkHead(html, {
        englishPath,
        lang: code,
        canonicalPath: localized,
        site,
        hreflangs: [
          ['en', englishPath],
          ['x-default', englishPath],
        ],
      });
      if (englishPath.startsWith('/docs') && !html.includes('data-pagefind-body')) {
        fail(`shipped translation ${localized} is not searchable`);
      }
    }
  }

  // Every locale page without a shipped translation falls back to English.
  // When every page is translated there are none here; the fixture build below
  // always removes one so the fallback path is still checked.
  const fallbacks = [];
  for (const locale of locales) {
    for (const englishPath of baseline.filter((route) => route === '/docs' || route.startsWith('/docs/'))) {
      const localized = `/${locale}${englishPath}`;
      if (shippedTranslations(englishPath).some(([, have]) => have === localized)) continue;
      fallbacks.push(localized);
      checkFallback(outDir, locale, englishPath, site);
    }
  }
  if (translated && !fallbacks.includes('/ja/docs/install')) fail('fixture build has no fallback /ja/docs/install');

  const switcher = readHtml(outDir, '/ja/docs/install');
  if (!switcher) fail('missing /ja/docs/install');
  else {
    if (!switcher.includes('>English<') || !switcher.includes('>日本語<')) fail('language switcher missing native names');
    if (!switcher.includes('href="/ja/docs/concepts"') && !switcher.includes('href="/ja/docs/concepts#')) {
      fail('docs nav did not stay on /ja');
    }
  }

  const englishConcepts = readHtml(outDir, '/docs/concepts');
  if (!englishConcepts) fail('missing /docs/concepts');
  else if (translated) {
    checkHead(englishConcepts, {
      englishPath: '/docs/concepts',
      lang: 'en',
      canonicalPath: '/docs/concepts',
      site,
      hreflangs: [
        ['en', '/docs/concepts'],
        ['ja', '/ja/docs/concepts'],
        ['x-default', '/docs/concepts'],
      ],
      absent: ['zh-CN', 'fr'],
    });
  } else {
    checkHead(englishConcepts, {
      englishPath: '/docs/concepts',
      lang: 'en',
      canonicalPath: '/docs/concepts',
      site,
      hreflangs: [
        ['en', '/docs/concepts'],
        ['x-default', '/docs/concepts'],
      ],
      absent: ['ja', 'zh-CN'],
    });
  }

  const sitemapFile = path.join(outDir, 'sitemap.xml');
  if (!fs.existsSync(sitemapFile)) fail('missing sitemap.xml');
  else {
    const xml = fs.readFileSync(sitemapFile, 'utf8');
    const locs = [...xml.matchAll(/<loc>([^<]+)<\/loc>/g)].map((match) => pathnameOf(match[1]));
    if (site) {
      const absolute = [
        ...xml.matchAll(/<loc>([^<]+)<\/loc>/g),
        ...xml.matchAll(/\bhref="([^"]+)"/g),
      ].map((match) => match[1]);
      if (!absolute.length || absolute.some((href) => !href.startsWith(`${site}/`))) {
        fail(`sitemap links are not all on ${site}`);
      }
    }
    if (!locs.includes('/docs/concepts')) fail('sitemap missing /docs/concepts');
    for (const localized of fallbacks) {
      if (locs.includes(localized)) fail(`sitemap includes fallback ${localized}`);
    }
    for (const englishPath of baseline.filter((route) => route.startsWith('/docs'))) {
      for (const [, localized] of shippedTranslations(englishPath)) {
        if (!locs.includes(localized)) fail(`sitemap missing translated ${localized}`);
      }
    }
    if (!xml.includes('hreflang="x-default"')) fail('sitemap missing x-default');
  }

  if (translated) {
    const page = readHtml(outDir, '/ja/docs/concepts');
    if (!page) fail('missing translated /ja/docs/concepts');
    else {
      checkHead(page, {
        englishPath: '/docs/concepts',
        lang: 'ja',
        canonicalPath: '/ja/docs/concepts',
        site,
        hreflangs: [
          ['en', '/docs/concepts'],
          ['ja', '/ja/docs/concepts'],
          ['x-default', '/docs/concepts'],
        ],
        absent: ['zh-CN', 'fr'],
      });
      if (!page.includes('Concepts fixture') || !page.includes('This is a Japanese fixture page.')) {
        fail('translated page did not render the fixture');
      }
      if (!page.includes('data-pagefind-body')) fail('translated docs page is not searchable');
    }
    const urls = indexedUrls(outDir);
    if (!urls.some((url) => url.includes('/ja/docs/concepts'))) fail('ja search index missing the fixture');

    const entry = JSON.parse(fs.readFileSync(path.join(outDir, 'pagefind', 'pagefind-entry.json'), 'utf8'));
    if (!entry.languages?.ja) fail('pagefind has no ja language index');
    if (entry.languages?.en?.page_count && urls.filter((url) => url.includes('/ja/')).some((url) => url.startsWith('/docs'))) {
      fail('english index contains a locale url');
    }
  }
}

if (!fs.existsSync(dist)) {
  console.error('No build output at dist. Run npm run build first.');
  process.exit(1);
}

const site = configuredSite();
checkBuild(dist, { translated: false, site });
const plain = readHtml(dist, '/ja/docs/concepts');
if (plain?.includes('Concepts fixture')) fail('production build contains the i18n fixture');

// The fixture build swaps in a known Japanese concepts page and takes the
// Japanese install page away, so both a translation and a fallback are checked
// even when every page ships translated. Real files are moved aside and put
// back afterwards; nothing in src/content is deleted.
const jaDocs = path.join(root, 'src/content/docs/ja');
const fixtureDest = path.join(jaDocs, 'concepts.md');
const fixtureSrc = path.join(root, 'scripts/fixtures/i18n/ja-concepts.md');
const fixtureOut = path.join(root, '.i18n-fixture-dist');
const stash = fs.mkdtempSync(path.join(root, '.i18n-stash-'));
const jaDocsExisted = fs.existsSync(jaDocs);
const swapped = ['concepts.md', 'install.md', 'install.mdx'].filter((name) => fs.existsSync(path.join(jaDocs, name)));
try {
  for (const name of swapped) fs.renameSync(path.join(jaDocs, name), path.join(stash, name));
  fs.mkdirSync(jaDocs, { recursive: true });
  fs.copyFileSync(fixtureSrc, fixtureDest);
  const build = spawnSync('npx', ['astro', 'build', '--outDir', fixtureOut], {
    cwd: root,
    env: process.env,
    stdio: 'inherit',
  });
  if (build.status !== 0) fail('fixture build failed');
  else checkBuild(fixtureOut, { translated: true, site });
} finally {
  fs.rmSync(fixtureDest, { force: true });
  for (const name of swapped) fs.renameSync(path.join(stash, name), path.join(jaDocs, name));
  if (!jaDocsExisted && fs.readdirSync(jaDocs).length === 0) fs.rmdirSync(jaDocs);
  fs.rmSync(stash, { recursive: true, force: true });
  fs.rmSync(fixtureOut, { recursive: true, force: true });
}

if (!swapped.includes('concepts.md') && fs.existsSync(fixtureDest)) fail('fixture file was left in src/content');
for (const name of swapped) {
  if (!fs.existsSync(path.join(jaDocs, name))) fail(`translation ${name} was not restored`);
}

if (failures.length) {
  console.error(`i18n check failed (${failures.length}):`);
  for (const failure of failures) console.error(`  ${failure}`);
  process.exit(1);
}

console.log('i18n check passed (English routes unchanged, fallback and fixture translation).');
