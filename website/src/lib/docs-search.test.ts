import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { describe, it } from 'node:test';

import {
  SEARCH_CLOSE,
  SEARCH_FOOTER,
  SEARCH_HINT,
  SEARCH_LABEL,
  SEARCH_PLACEHOLDER,
  moveResultIndex,
  noResultsMessage,
  resultHref,
  searchRows,
  slashShouldFocus,
} from './docs-search.ts';

describe('docs search copy', () => {
  it('keeps the content-locked strings', () => {
    assert.equal(SEARCH_PLACEHOLDER, 'Search docs');
    assert.equal(SEARCH_LABEL, 'Search docs');
    assert.equal(SEARCH_HINT, '/');
    assert.equal(SEARCH_FOOTER, '↑↓ to move, Enter to open, Esc to close');
    assert.equal(SEARCH_CLOSE, 'Close');
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

  it('shows the page title, a distinct section, and the snippet', () => {
    const rows = searchRows(
      [
        {
          url: '/docs/install.html',
          excerpt: 'page excerpt',
          meta: { title: 'Install' },
          sub_results: [
            {
              title: 'Install',
              url: '/docs/install.html',
              excerpt: 'Install Coppice on <mark>macOS</mark>.',
            },
            {
              title: 'Windows 11 (through WSL)',
              url: '/docs/install.html#windows-11-through-wsl',
              excerpt: '<mark>Windows</mark> 11 through WSL.',
            },
          ],
        },
        {
          url: '/docs/faq.html',
          excerpt: 'faq page',
          meta: { title: ' FAQ ' },
          sub_results: [{ title: 'Windows?', url: '/docs/faq.html#windows', excerpt: 'See WSL.' }],
        },
      ],
      2,
    );

    assert.deepEqual(rows, [
      {
        url: '/docs/install.html',
        pageTitle: 'Install',
        section: '',
        excerpt: 'Install Coppice on <mark>macOS</mark>.',
      },
      {
        url: '/docs/install.html#windows-11-through-wsl',
        pageTitle: 'Install',
        section: 'Windows 11 (through WSL)',
        excerpt: '<mark>Windows</mark> 11 through WSL.',
      },
    ]);
  });

  it('falls back to the page excerpt when a hit has no sections', () => {
    assert.deepEqual(
      searchRows([{ url: '/docs/plugins.html', excerpt: 'Plugin body', meta: { title: 'Plugins' } }], 8),
      [{ url: '/docs/plugins.html', pageTitle: 'Plugins', section: '', excerpt: 'Plugin body' }],
    );
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
    assert.match(search, /class="docs-search-open"/);
    assert.match(search, /class="docs-search-open-label">\{SEARCH_LABEL\}<\/span>/);
    assert.match(search, /aria-label=\{SEARCH_LABEL\}/);
    assert.match(
      search,
      /id="docs-search-dialog"[\s\S]*?role="dialog"[\s\S]*?aria-modal="true"[\s\S]*?aria-label=\{SEARCH_LABEL\}/,
    );
    assert.match(search, /class="docs-search-backdrop"/);
    assert.match(search, /class="docs-search-close"[^>]*>\{SEARCH_CLOSE\}<\/button>/);
    assert.match(search, /class="docs-search-keys">\{SEARCH_FOOTER\}<\/p>/);
    assert.match(search, /searchRows\(/);
    assert.doesNotMatch(search, /<h[1-6]\b/);
    const script = search.slice(search.indexOf('<script>'));
    assert.match(script, /function openDialog\([\s\S]*input\.focus\(\)/);
    assert.match(script, /function closeDialog\([\s\S]*\.focus\(\)/);
    assert.match(script, /slashShouldFocus[\s\S]*openDialog\(\)/);
    assert.match(script, /Escape[\s\S]*closeDialog\(/);
    assert.match(script, /docs-search-backdrop[\s\S]*closeDialog\(/);
    assert.match(script, /docs-search-close[\s\S]*closeDialog\(/);
    assert.match(read('layouts/Docs.astro'), /<DocsSearch\b/);
    assert.match(css, /\.docs-search-hint \{\n  display: none;\n\}/);
    assert.match(css, /@media \(min-width: 880px\) \{[\s\S]*?\.docs-search-hint \{\n    display: inline-flex;/);
    assert.match(css, /\.docs-search-dialog \{\n  position: fixed;\n  inset: 0;/);
    assert.match(css, /\.docs-search-dialog\[hidden\] \{\n  display: none;\n\}/);
    assert.match(css, /place-items: center;/);
    assert.match(css, /\.docs-search-close \{\n  display: none;\n\}/);
    assert.match(css, /\.docs-search-open \{\n(?:  .*\n)*?  border-radius: 999px;\n/);
    assert.match(css, /@media \(max-width: 640px\) \{[\s\S]*?\.docs-search-open-label \{\n    display: none;\n/);
    assert.match(css, /@media \(max-width: 640px\) \{[\s\S]*?\.docs-search-keys \{\n    display: none;\n/);
    assert.match(css, /@media \(max-width: 640px\) \{[\s\S]*?\.docs-search-close \{\n    display: inline-flex;\n/);
    assert.match(
      css,
      /@media \(max-width: 640px\) \{[\s\S]*?\.docs-search-modal \{\n    width: 100%;\n    height: 100%;\n/,
    );
  });
});
