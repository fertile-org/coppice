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

const htmlFiles = walk(dist);
const marketing = new Set(['index.html', 'download.html', '404.html']);
const docsFiles = htmlFiles.filter((file) => {
  const name = rel(file);
  return name === 'docs.html' || name.startsWith('docs/');
});

if (docsFiles.length === 0) failures.push('no docs html in dist');

for (const file of htmlFiles) {
  const name = rel(file);
  const html = fs.readFileSync(file, 'utf8');
  if (!marketing.has(name)) continue;
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

for (const file of docsFiles) {
  const name = rel(file);
  const html = fs.readFileSync(file, 'utf8');
  if (!html.includes('<article class="prose" data-pagefind-body>')) {
    failures.push(`${name} article is missing data-pagefind-body`);
  }
  if (!html.includes('<nav class="docs-nav" data-pagefind-ignore')) {
    failures.push(`${name} docs nav is missing data-pagefind-ignore`);
  }
  if (!html.includes('placeholder="Search docs"')) failures.push(`${name} missing search placeholder`);
  if (!html.includes('aria-label="Search docs"')) failures.push(`${name} missing search label`);
  if (!html.includes('<label class="visually-hidden" for="docs-search">Search docs</label>')) {
    failures.push(`${name} missing accessible search label`);
  }
  if (!html.includes('<kbd class="docs-search-hint" aria-hidden="true">/</kbd>')) {
    failures.push(`${name} missing shortcut hint`);
  }
  if (!html.includes('class="docs-search-open"')) failures.push(`${name} missing search button`);
  if (!html.includes('role="dialog"')) failures.push(`${name} missing search dialog`);
  if (!html.includes('aria-modal="true"')) failures.push(`${name} search dialog missing aria-modal`);
  if (!html.includes('>Search docs</span>')) failures.push(`${name} missing search button text`);
  if (!html.includes('↑↓ to move, Enter to open, Esc to close')) failures.push(`${name} missing search footer hint`);
  if (!html.includes('>Close</button>')) failures.push(`${name} missing search close button`);
}

const entryPath = path.join(dist, 'pagefind', 'pagefind-entry.json');
if (!fs.existsSync(path.join(dist, 'pagefind', 'pagefind.js'))) failures.push('missing pagefind/pagefind.js');
if (!fs.existsSync(entryPath)) failures.push('missing pagefind/pagefind-entry.json');
else {
  const entry = JSON.parse(fs.readFileSync(entryPath, 'utf8'));
  const count = entry.languages?.en?.page_count;
  if (count !== docsFiles.length) {
    failures.push(`pagefind indexed ${count} pages, expected ${docsFiles.length} docs pages`);
  }
}

const fragmentDir = path.join(dist, 'pagefind', 'fragment');
const urls = [];
const banned = [
  'Put your agents on a board',
  'Run a team of coding agents without babysitting them',
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
    if (!record.url.startsWith('/docs')) failures.push(`indexed non-docs url ${record.url}`);
  }
}

const expected = docsFiles.map((file) => `/${rel(file)}`).sort();
const actual = [...urls].sort();
if (actual.length && JSON.stringify(actual) !== JSON.stringify(expected)) {
  failures.push(`indexed urls ${actual.join(', ')} != docs html ${expected.join(', ')}`);
}

if (failures.length) {
  console.error(`Docs search index check failed (${failures.length}):`);
  for (const failure of failures) console.error(`  ${failure}`);
  process.exit(1);
}

console.log(`Docs search index check passed (${docsFiles.length} docs pages).`);
