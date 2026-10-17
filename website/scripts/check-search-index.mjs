import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';

const dist = path.resolve('dist');
const failures = [];

function walk(dir, out = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === 'pagefind' || entry.name === '_astro') continue;
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full, out);
    else if (entry.name.endsWith('.html')) out.push(full);
  }
  return out;
}

function rel(file) {
  return path.relative(dist, file).split(path.sep).join('/');
}

if (!fs.existsSync(dist)) {
  console.error('No build output at dist. Run astro build first.');
  process.exit(1);
}

function isRedirect(html) {
  return /http-equiv=["']refresh["']/i.test(html) && !html.includes('data-pagefind-body');
}

const LOCALE_PATHS = ['zh-cn', 'zh-tw', 'zh-hk', 'ja', 'fr'];

function routeOf(file) {
  const name = rel(file).replace(/\.html$/, '');
  return name === 'index' ? '/' : `/${name}`;
}

function splitRoute(route) {
  const segment = route.split('/')[1] ?? '';
  if (LOCALE_PATHS.includes(segment)) {
    const rest = route.slice(segment.length + 1);
    return { locale: segment, englishPath: rest || '/' };
  }
  return { locale: 'en', englishPath: route };
}

function searchPlaceholder(locale) {
  if (locale === 'en') return 'Search docs';
  const file = path.resolve('src/i18n/ui', `${locale}.ts`);
  if (!fs.existsSync(file)) return 'Search docs';
  const match = fs.readFileSync(file, 'utf8').match(/placeholder:\s*(?:'|")([^'"]*)(?:'|")/);
  return match?.[1] || 'Search docs';
}

const htmlFiles = walk(dist);
const pages = htmlFiles.map((file) => {
  const html = fs.readFileSync(file, 'utf8');
  const route = routeOf(file);
  return { file, name: rel(file), html, route, ...splitRoute(route), redirect: isRedirect(html) };
});
const marketingPaths = new Set(['/', '/download', '/404']);
const marketing = pages.filter((page) => marketingPaths.has(page.englishPath));
const docsPages = pages.filter(
  (page) => (page.englishPath === '/docs' || page.englishPath.startsWith('/docs/')) && !page.redirect,
);
const indexedDocs = docsPages.filter((page) => page.html.includes('<article class="prose" data-pagefind-body>'));

if (!docsPages.some((page) => page.locale === 'en')) failures.push('no English docs html in dist');

for (const page of marketing) {
  const { name, html } = page;
  if (html.includes('data-pagefind-body')) failures.push(`${name} includes data-pagefind-body`);
  if (!html.includes('<header class="site-header" data-pagefind-ignore>')) {
    failures.push(`${name} header is missing data-pagefind-ignore`);
  }
  if (!html.includes('<nav class="primary-nav" data-pagefind-ignore')) {
    failures.push(`${name} primary nav is missing data-pagefind-ignore`);
  }
  if (!html.includes('<footer class="site-footer" data-pagefind-ignore>')) {
    failures.push(`${name} footer is missing data-pagefind-ignore`);
  }
  if (!html.includes('<nav data-pagefind-ignore aria-label="Footer">')) {
    failures.push(`${name} footer nav is missing data-pagefind-ignore`);
  }
  if (html.includes('docs-search')) failures.push(`${name} includes the docs search box`);
}

for (const page of docsPages) {
  const { name, html, locale } = page;
  const indexed = html.includes('<article class="prose" data-pagefind-body>');
  if (locale === 'en' && !indexed) failures.push(`${name} article is missing data-pagefind-body`);
  if (locale !== 'en' && indexed && !html.includes(`lang="${locale === 'zh-cn' ? 'zh-CN' : locale === 'zh-tw' ? 'zh-TW' : locale === 'zh-hk' ? 'zh-HK' : locale}"`)) {
    failures.push(`${name} translated docs page is missing its html lang`);
  }
  if (!indexed && html.includes('data-pagefind-body')) {
    failures.push(`${name} fallback docs page is indexed`);
  }
  if (!html.includes('<nav class="docs-nav" data-pagefind-ignore')) {
    failures.push(`${name} docs nav is missing data-pagefind-ignore`);
  }
  const placeholder = searchPlaceholder(locale);
  if (!html.includes(`placeholder="${placeholder}"`)) failures.push(`${name} missing search placeholder`);
  if (!html.includes('class="docs-search-open"')) failures.push(`${name} missing search button`);
  if (!html.includes('role="dialog"')) failures.push(`${name} missing search dialog`);
  if (!html.includes('aria-modal="true"')) failures.push(`${name} search dialog missing aria-modal`);
  if (locale !== 'en') continue;
  if (!html.includes('aria-label="Search docs"')) failures.push(`${name} missing search label`);
  if (!html.includes('<label class="visually-hidden" for="docs-search">Search docs</label>')) {
    failures.push(`${name} missing accessible search label`);
  }
  if (!html.includes('<kbd class="docs-search-hint" aria-hidden="true">/</kbd>')) {
    failures.push(`${name} missing shortcut hint`);
  }
  if (!html.includes('>Search docs</span>')) failures.push(`${name} missing search button text`);
  if (!html.includes('↑↓ to move, Enter to open, Esc to close')) failures.push(`${name} missing search footer hint`);
  if (!html.includes('>Close</button>')) failures.push(`${name} missing search close button`);
}

const entryPath = path.join(dist, 'pagefind', 'pagefind-entry.json');
if (!fs.existsSync(path.join(dist, 'pagefind', 'pagefind.js'))) failures.push('missing pagefind/pagefind.js');
if (!fs.existsSync(entryPath)) failures.push('missing pagefind/pagefind-entry.json');
else {
  const entry = JSON.parse(fs.readFileSync(entryPath, 'utf8'));
  const expectedByLang = {};
  for (const page of indexedDocs) {
    const match = page.html.match(/<html[^>]*\blang="([^"]+)"/i);
    const key = (match?.[1] || 'en').toLowerCase();
    expectedByLang[key] = (expectedByLang[key] || 0) + 1;
  }
  const languages = entry.languages || {};
  for (const [key, info] of Object.entries(languages)) {
    if (info.page_count !== (expectedByLang[key] || 0)) {
      failures.push(`pagefind ${key} indexed ${info.page_count} pages, expected ${expectedByLang[key] || 0}`);
    }
  }
  for (const key of Object.keys(expectedByLang)) {
    if (!languages[key]) failures.push(`pagefind is missing the ${key} index`);
  }
}

const fragmentDir = path.join(dist, 'pagefind', 'fragment');
const urls = [];
const banned = [
  'Hire specialists. Keep the final say.',
  'Search docs',
  'Page not found',
];

if (!fs.existsSync(fragmentDir)) failures.push('missing pagefind fragments');
else {
  for (const name of fs.readdirSync(fragmentDir)) {
    if (!name.endsWith('.pf_fragment')) continue;
    const raw = zlib.gunzipSync(fs.readFileSync(path.join(fragmentDir, name)));
    const jsonStart = raw.indexOf('{');
    const record = JSON.parse(raw.subarray(jsonStart).toString('utf8'));
    urls.push(record.url);
    for (const phrase of banned) {
      if (String(record.content || '').includes(phrase)) {
        failures.push(`index contains ${JSON.stringify(phrase)} via ${record.url}`);
      }
    }
    const indexedPath = record.url.startsWith('/') ? record.url : `/${record.url}`;
    const localeSegment = indexedPath.split('/')[1];
    const englishUrl = LOCALE_PATHS.includes(localeSegment)
      ? indexedPath.slice(localeSegment.length + 1) || '/'
      : indexedPath;
    if (!(englishUrl === '/docs' || englishUrl.startsWith('/docs/') || englishUrl.startsWith('/docs.'))) {
      failures.push(`indexed non-docs url ${record.url}`);
    }
  }
}

const expected = indexedDocs.map((page) => `/${page.name}`).sort();
const actual = [...urls].sort();
if (actual.length && JSON.stringify(actual) !== JSON.stringify(expected)) {
  failures.push(`indexed urls ${actual.join(', ')} != docs html ${expected.join(', ')}`);
}

if (failures.length) {
  console.error(`Docs search index check failed (${failures.length}):`);
  for (const failure of failures) console.error(`  ${failure}`);
  process.exit(1);
}

console.log(`Docs search index check passed (${indexedDocs.length} indexed docs pages, ${docsPages.length} docs html files).`);
