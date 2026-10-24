import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import { AGENT_CLIS } from '../content/agents.ts';
import { breadcrumbList, ids, jsonLd, organization, softwareApplication, techArticle } from './structured-data.ts';

const ref = { site: 'https://example.com' };

describe('structured data', () => {
  it('links the app and articles to one organization by @id', () => {
    assert.equal(organization(ref)['@id'], 'https://example.com/#organization');
    const app = softwareApplication(ref, { description: 'd', inLanguage: 'en', image: 'i', screenshot: 's' });
    assert.deepEqual(app.publisher, { '@id': ids.organization(ref) });
    assert.deepEqual(app.offers, { '@type': 'Offer', price: '0', priceCurrency: 'USD' });
    for (const name of AGENT_CLIS) assert.match(String(app.keywords), new RegExp(name));
    const article = techArticle(ref, {
      headline: 'Install',
      description: 'd',
      pathname: '/ja/docs/install',
      inLanguage: 'ja',
      image: 'i',
    });
    assert.equal(article.url, 'https://example.com/ja/docs/install');
    assert.deepEqual(article.about, { '@id': 'https://example.com/#software' });
    assert.equal('dateModified' in article, false);
    assert.equal('keywords' in article, false);
  });

  it('numbers breadcrumbs and makes their URLs absolute under the base', () => {
    const list = breadcrumbList({ site: 'https://example.com', base: '/coppice' }, [
      { name: 'Coppice', pathname: '/' },
      { name: 'Docs', pathname: '/docs' },
    ]);
    assert.deepEqual(list.itemListElement, [
      { '@type': 'ListItem', position: 1, name: 'Coppice', item: 'https://example.com/coppice' },
      { '@type': 'ListItem', position: 2, name: 'Docs', item: 'https://example.com/coppice/docs' },
    ]);
  });

  it('escapes < so a value cannot end the script element', () => {
    const out = jsonLd([{ '@type': 'Thing', name: '</script><b>' }]);
    assert.doesNotMatch(out, /<\/script>/);
    assert.equal(JSON.parse(out)['@graph'][0].name, '</script><b>');
  });
});
