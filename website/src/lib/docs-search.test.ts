import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { describe, it } from 'node:test';

import {
  SEARCH_HINT,
  SEARCH_LABEL,
  SEARCH_PLACEHOLDER,
  moveResultIndex,
  noResultsMessage,
  resultHref,
  slashShouldFocus,
} from './docs-search.ts';

describe('docs search copy', () => {
  it('keeps the content-locked strings', () => {
    assert.equal(SEARCH_PLACEHOLDER, 'Search docs');
    assert.equal(SEARCH_LABEL, 'Search docs');
    assert.equal(SEARCH_HINT, '/');
    assert.equal(noResultsMessage('install'), 'No results for "install".');
    assert.equal(noResultsMessage('a "quote"'), 'No results for "a "quote"".');
  });
});

describe('docs search shortcut', () => {
  it('focuses search on slash when the user is not typing in a field', () => {
    assert.equal(slashShouldFocus({ key: '/', target: { tagName: 'BODY' } }), true);
    assert.equal(slashShouldFocus({ key: '/', target: null }), true);
    assert.equal(slashShouldFocus({ key: '?', target: { tagName: 'BODY' } }), false);
    assert.equal(slashShouldFocus({ key: '/', metaKey: true, target: { tagName: 'BODY' } }), false);
    assert.equal(slashShouldFocus({ key: '/', ctrlKey: true, target: { tagName: 'BODY' } }), false);
    assert.equal(slashShouldFocus({ key: '/', altKey: true, target: { tagName: 'BODY' } }), false);
  });

  it('ignores slash while typing in a field', () => {
    for (const tagName of ['INPUT', 'TEXTAREA', 'SELECT', 'input']) {
      assert.equal(slashShouldFocus({ key: '/', target: { tagName } }), false);
    }
    assert.equal(slashShouldFocus({ key: '/', target: { tagName: 'DIV', isContentEditable: true } }), false);
  });
});

describe('docs search results', () => {
  it('moves through results from the keyboard', () => {
    assert.equal(moveResultIndex(-1, 3, 1), 0);
    assert.equal(moveResultIndex(-1, 3, -1), 2);
    assert.equal(moveResultIndex(0, 3, -1), 2);
    assert.equal(moveResultIndex(2, 3, 1), 0);
    assert.equal(moveResultIndex(0, 0, 1), -1);
  });

  it('points result links at clean docs urls', () => {
    assert.equal(resultHref('/docs/install.html', '/'), '/docs/install');
    assert.equal(resultHref('/docs/install.html#windows-11-through-wsl', '/'), '/docs/install#windows-11-through-wsl');
    assert.equal(resultHref('/docs.html', '/'), '/docs');
    assert.equal(resultHref('/docs/install.html', '/coppice/'), '/coppice/docs/install');
    assert.equal(resultHref('/coppice/docs/install.html', '/coppice/'), '/coppice/docs/install');
  });
});

const src = path.resolve('src');

function read(relative: string): string {
  return fs.readFileSync(path.join(src, relative), 'utf8');
}

describe('docs search markup', () => {
  it('indexes only the docs article and ignores site chrome', () => {
    const docs = read('layouts/Docs.astro');
    const header = read('components/SiteHeader.astro');
    const footer = read('components/SiteFooter.astro');
    const home = read('pages/index.astro');

    assert.match(docs, /<article\b[^>]*\bdata-pagefind-body\b/);
    assert.match(docs, /<nav\b[^>]*\bdata-pagefind-ignore\b/);
    assert.match(header, /<header\b[^>]*\bdata-pagefind-ignore\b/);
    assert.match(header, /<nav\b[^>]*\bdata-pagefind-ignore\b/);
    assert.match(footer, /<footer\b[^>]*\bdata-pagefind-ignore\b/);
    assert.match(footer, /<nav\b[^>]*\bdata-pagefind-ignore\b/);
    assert.doesNotMatch(home, /data-pagefind-body/);
    assert.doesNotMatch(read('pages/download.astro'), /data-pagefind-body/);
    assert.doesNotMatch(read('pages/404.astro'), /data-pagefind-body/);
  });

  it('puts the locked search strings in the docs header', () => {
    const search = read('components/DocsSearch.astro');
    const css = read('styles/global.css');
    assert.match(search, /placeholder=\{SEARCH_PLACEHOLDER\}/);
    assert.match(search, /aria-label=\{SEARCH_LABEL\}/);
    assert.match(search, />\{SEARCH_LABEL\}</);
    assert.match(search, /noResultsMessage\(/);
    assert.match(search, /<kbd class="docs-search-hint"[^>]*>\{SEARCH_HINT\}<\/kbd>/);
    assert.match(read('layouts/Docs.astro'), /<DocsSearch\b/);
    assert.match(css, /\.docs-search-hint \{\n  display: none;\n\}/);
    assert.match(css, /@media \(min-width: 880px\) \{[\s\S]*?\.docs-search-hint \{\n    display: inline-flex;/);
  });
});
