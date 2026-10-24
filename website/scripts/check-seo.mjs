// SEO checks on the built site. Run after `npm run build`.
// Every page: one title, one description, one canonical on `site`, Open Graph and
// Twitter tags that agree with it, and an og:image that exists in dist.
// Indexable pages (self canonical, no noindex): one h1 and a title unique within
// their language. Also robots.txt, the sitemap, the manifest, and any JSON-LD.
import fs from 'node:fs';
import path from 'node:path';

const root = process.cwd();
const dist = path.resolve('dist');
const failures = [];
const TITLE_MAX = 60;

function fail(message) {
  failures.push(message);
}

if (!fs.existsSync(dist)) {
  console.error(`No build output at ${dist}. Run npm run build first.`);
  process.exit(1);
}

function walk(dir, out = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === 'pagefind' || entry.name === '_astro') continue;
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full, out);
    else if (entry.name.endsWith('.html')) out.push(full);
  }
  return out;
}

function routeOf(file) {
  let rel = path.relative(dist, file).split(path.sep).join('/');
  if (rel.endsWith('/index.html')) rel = rel.slice(0, -'/index.html'.length);
  else if (rel.endsWith('.html')) rel = rel.slice(0, -'.html'.length);
  if (rel === 'index' || rel === '') return '/';
  return `/${rel}`;
}

/** The origin in `astro.config.mjs` `site`. */
function configuredSite() {
  const source = fs.readFileSync(path.join(root, 'astro.config.mjs'), 'utf8');
  const site = source.match(/\bsite:\s*['"](https?:\/\/[^'"]+)['"]/)?.[1];
  if (!site) throw new Error('astro.config.mjs is missing site');
  return site.replace(/\/$/, '');
}

const decode = (value) =>
  value
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&');

function tags(html, name) {
  return [...html.matchAll(new RegExp(`<${name}\\b[^>]*>`, 'gi'))].map((match) => match[0]);
}

function attrOf(tag, name) {
  const value = tag.match(new RegExp(`\\b${name}=(?:"([^"]*)"|'([^']*)')`, 'i'));
  return value ? decode(value[1] ?? value[2]) : undefined;
}

/** Values of `<meta name|property=key content>`. */
function meta(html, key) {
  return tags(html, 'meta')
    .filter((tag) => attrOf(tag, 'name') === key || attrOf(tag, 'property') === key)
    .map((tag) => attrOf(tag, 'content') ?? '');
}

function links(html, rel) {
  return tags(html, 'link')
    .filter((tag) => attrOf(tag, 'rel') === rel)
    .map((tag) => attrOf(tag, 'href') ?? '');
}

/** dist file for a site URL, or null when it is not one of ours. */
function distFileFor(href, site) {
  if (!href.startsWith(`${site}/`) && href !== site) return null;
  let pathname = decodeURIComponent(new URL(href).pathname);
  if (pathname === '/') return path.join(dist, 'index.html');
  pathname = pathname.replace(/\/$/, '');
  for (const candidate of [pathname, `${pathname}.html`, `${pathname}/index.html`]) {
    const file = path.join(dist, candidate);
    if (fs.existsSync(file) && fs.statSync(file).isFile()) return file;
  }
  return null;
}

const site = configuredSite();
const titlesByLang = new Map();
const indexable = new Set();

for (const file of walk(dist)) {
  const route = routeOf(file);
  const html = fs.readFileSync(file, 'utf8');
  if (/<meta[^>]+http-equiv=["']refresh["']/i.test(html)) continue; // Astro redirect stubs

  const where = (message) => fail(`${route}: ${message}`);
  const lang = html.match(/<html[^>]*\blang=["']([^"']+)["']/i)?.[1] ?? '';
  const noindex = meta(html, 'robots').some((value) => /noindex/i.test(value));
  const titles = [...html.matchAll(/<title>([^<]*)<\/title>/gi)].map((match) => decode(match[1]).trim());
  const descriptions = meta(html, 'description');
  const canonicals = links(html, 'canonical');

  if (!lang) where('missing <html lang>');
  if (titles.length !== 1 || !titles[0]) where(`expected one non-empty <title>, found ${titles.length}`);
  if (descriptions.length !== 1 || !descriptions[0].trim()) where('expected one non-empty meta description');
  if (route === '/404' && !noindex) where('404 must be noindex');
  if (noindex) continue;

  if (canonicals.length !== 1) {
    where(`expected one canonical, found ${canonicals.length}`);
    continue;
  }
  const canonical = canonicals[0];
  if (!canonical.startsWith(site)) where(`canonical ${canonical} is not on ${site}`);

  const one = (key) => {
    const values = meta(html, key);
    if (values.length !== 1 || !values[0]) where(`expected one ${key}, found ${values.length}`);
    return values[0] ?? '';
  };
  if (one('og:url') !== canonical) where(`og:url does not match canonical ${canonical}`);
  if (one('og:title') !== titles[0]) where('og:title does not match <title>');
  if (one('og:description') !== descriptions[0]) where('og:description does not match meta description');
  one('og:type');
  one('og:site_name');
  one('og:locale');
  one('og:image:alt');
  if (one('twitter:card') !== 'summary_large_image') where('twitter:card should be summary_large_image');
  const image = one('og:image');
  if (image && !distFileFor(image, site)) where(`og:image ${image} is not in dist`);
  if (image && one('twitter:image') !== image) where('twitter:image does not match og:image');
  if (one('og:image:width') !== '1200' || one('og:image:height') !== '630') where('og:image should be 1200×630');

  const schema = new Map();
  for (const [index, match] of [...html.matchAll(/<script[^>]*type=["']application\/ld\+json["'][^>]*>([\s\S]*?)<\/script>/gi)].entries()) {
    try {
      const data = JSON.parse(match[1]);
      const items = Array.isArray(data) ? data : data['@graph'] ?? [data];
      if (!JSON.stringify(data).includes('https://schema.org')) where(`JSON-LD #${index + 1} has no schema.org @context`);
      for (const item of items) {
        if (!item['@type']) where(`JSON-LD #${index + 1} has an item without @type`);
        else schema.set(item['@type'], item);
      }
    } catch (error) {
      where(`JSON-LD #${index + 1} does not parse: ${error.message}`);
    }
  }

  // Locale URLs still showing English point their canonical at the English page.
  const selfCanonical = distFileFor(canonical, site) === file;
  if (!selfCanonical) continue;
  indexable.add(canonical);

  const englishRoute = route.replace(/^\/(vi|zh-cn|zh-tw|zh-hk|ja|fr)(?=\/|$)/, '') || '/';
  const required = ['Organization', 'WebSite'];
  if (englishRoute === '/' || englishRoute === '/download') required.push('SoftwareApplication');
  if (englishRoute === '/docs' || englishRoute.startsWith('/docs/')) required.push('TechArticle', 'BreadcrumbList');
  for (const type of required) if (!schema.has(type)) where(`JSON-LD is missing ${type}`);
  const app = schema.get('SoftwareApplication');
  if (app) {
    for (const key of ['name', 'operatingSystem', 'applicationCategory', 'offers']) {
      if (!app[key]) where(`SoftwareApplication is missing ${key}`);
    }
    if (app.offers && app.offers.price === undefined) where('SoftwareApplication offers has no price');
  }
  const article = schema.get('TechArticle');
  if (article && article.url !== canonical) where(`TechArticle url ${article.url} is not the canonical ${canonical}`);
  if (article && article.inLanguage !== lang) where(`TechArticle inLanguage ${article.inLanguage} is not <html lang> ${lang}`);
  const crumbs = schema.get('BreadcrumbList')?.itemListElement ?? [];
  for (const crumb of crumbs) {
    if (!crumb.item || !distFileFor(crumb.item, site)) where(`breadcrumb ${crumb.name} points at ${crumb.item}, which is not a page`);
  }

  if (titles[0] && titles[0].length > TITLE_MAX) where(`title is ${titles[0].length} characters (max ${TITLE_MAX}): ${titles[0]}`);
  const h1s = (html.match(/<h1\b/gi) ?? []).length;
  if (h1s !== 1) where(`expected one <h1>, found ${h1s}`);
  const seen = titlesByLang.get(lang) ?? new Map();
  if (titles[0] && seen.has(titles[0])) where(`title "${titles[0]}" is also used by ${seen.get(titles[0])}`);
  else if (titles[0]) seen.set(titles[0], route);
  titlesByLang.set(lang, seen);
}

const robotsFile = path.join(dist, 'robots.txt');
if (!fs.existsSync(robotsFile)) {
  fail('dist/robots.txt is missing');
} else {
  const robots = fs.readFileSync(robotsFile, 'utf8');
  if (!robots.includes(`Sitemap: ${site}/sitemap.xml`)) fail(`robots.txt must list Sitemap: ${site}/sitemap.xml`);
  if (/^Disallow:\s*\/\s*$/m.test(robots)) fail('robots.txt disallows the whole site');
}

const sitemapFile = path.join(dist, 'sitemap.xml');
if (!fs.existsSync(sitemapFile)) {
  fail('dist/sitemap.xml is missing');
} else {
  const sitemap = fs.readFileSync(sitemapFile, 'utf8');
  const locs = [...sitemap.matchAll(/<loc>([^<]+)<\/loc>/g)].map((match) => decode(match[1]));
  for (const loc of locs) {
    if (!indexable.has(loc)) fail(`sitemap lists ${loc}, which is not an indexable page with a self canonical`);
  }
  for (const loc of indexable) {
    if (!locs.includes(loc)) fail(`${loc} is indexable but missing from the sitemap`);
  }
  for (const date of sitemap.matchAll(/<lastmod>([^<]+)<\/lastmod>/g)) {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(date[1])) fail(`sitemap lastmod ${date[1]} is not YYYY-MM-DD`);
  }
}

const manifestFile = path.join(dist, 'site.webmanifest');
try {
  const manifest = JSON.parse(fs.readFileSync(manifestFile, 'utf8'));
  if (!manifest.name || !manifest.icons?.length) fail('site.webmanifest needs name and icons');
  for (const icon of manifest.icons ?? []) {
    if (!fs.existsSync(path.join(dist, icon.src))) fail(`site.webmanifest icon ${icon.src} is not in dist`);
  }
} catch (error) {
  fail(`site.webmanifest: ${error.message}`);
}

if (failures.length) {
  console.error(`SEO check failed (${failures.length}):`);
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log(`SEO check passed (${indexable.size} indexable pages).`);
