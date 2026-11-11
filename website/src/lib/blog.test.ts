import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import { buildCatalog } from '../i18n/catalog.ts';
import { sitemapXml, translatedLocales } from '../i18n/paths.ts';
import {
  BLOG_INDEX,
  blogLastmod,
  blogLocalesForSlug,
  collectBlogPosts,
  publishedTranslationSources,
  displayWidth,
  englishArticlePaths,
  formatBlogDate,
  publicPosts,
  renderedTitle,
  validateBlogSource,
} from './blog.ts';
import { blogHeadingErrors, isBlogMarkdown, transformBlogImages } from './blog-figures.mjs';
import { ui as frenchUi } from '../i18n/ui/fr.ts';
import { ui as japaneseUi } from '../i18n/ui/ja.ts';
import { ui as vietnameseUi } from '../i18n/ui/vi.ts';
import { ui as simplifiedUi } from '../i18n/ui/zh-cn.ts';
import { ui as hongKongUi } from '../i18n/ui/zh-hk.ts';
import { ui as taiwanUi } from '../i18n/ui/zh-tw.ts';

const DESCRIPTION = BLOG_INDEX.description;

function post(locale: string, slug: string, extra: Record<string, unknown> = {}) {
  return {
    file: `src/content/blog/${locale}/${slug}.md`,
    data: {
      title: 'A note',
      description: DESCRIPTION,
      answer: 'Assigning a repository and an agent does not replace the Run agent button in this note.',
      published: '2026-03-02',
      author: 'Fixture Author',
      ...extra,
    },
  };
}

describe('blog index copy', () => {
  it('keeps every locale index description inside the search-result width', () => {
    const width = displayWidth(BLOG_INDEX.description);
    assert.ok(width >= 100 && width <= 160, `en description ${width}`);
    assert.ok(displayWidth(renderedTitle(BLOG_INDEX.title)) <= 60);
    const catalogs = [
      ['fr', frenchUi],
      ['ja', japaneseUi],
      ['vi', vietnameseUi],
      ['zh-cn', simplifiedUi],
      ['zh-hk', hongKongUi],
      ['zh-tw', taiwanUi],
    ] as const;
    for (const [locale, ui] of catalogs) {
      const localeWidth = displayWidth(ui.blog.description);
      assert.ok(localeWidth >= 100 && localeWidth <= 160, `${locale} description ${localeWidth}`);
      assert.ok(displayWidth(renderedTitle(ui.nav.blog)) <= 60, `${locale} title`);
    }
  });
});

describe('blog frontmatter', () => {
  it('accepts a quoted date and a UTC midnight Date', () => {
    const quoted = validateBlogSource(post('en', 'worktrees').file, post('en', 'worktrees').data);
    assert.equal(quoted.errors.length, 0);
    assert.equal(quoted.post?.published, '2026-03-02');
    assert.equal(quoted.post?.authorType, 'Person');
    assert.equal(quoted.post?.englishPath, '/blog/worktrees');
    const dated = validateBlogSource(post('en', 'worktrees').file, {
      ...post('en', 'worktrees').data,
      published: new Date('2026-03-02T00:00:00.000Z'),
      updated: '2026-03-04',
      authorType: 'Organization',
    });
    assert.equal(dated.errors.length, 0);
    assert.equal(dated.post?.published, '2026-03-02');
    assert.equal(dated.post?.updated, '2026-03-04');
    assert.equal(dated.post?.authorType, 'Organization');
  });

  it('rejects invented defaults: missing author, bad dates, and unknown keys', () => {
    const result = validateBlogSource('src/content/blog/en/worktrees.md', {
      title: 'A note',
      description: DESCRIPTION,
      answer: 'Short.',
      published: '2026-02-31',
      updated: '2026-01-01',
      seoTraffic: 'high',
    });
    assert.match(result.errors.join('\n'), /author is required/);
    assert.match(result.errors.join('\n'), /answer must be/);
    assert.match(result.errors.join('\n'), /published must be a real/);
    assert.match(result.errors.join('\n'), /unknown frontmatter key seoTraffic/);
  });

  it('rejects a translation that has no English article', () => {
    const { errors } = collectBlogPosts([post('ja', 'worktrees')]);
    assert.match(errors.join('\n'), /no English article/);
  });

  it('hides drafts and lists only locales that have a public file', () => {
    const { posts, errors } = collectBlogPosts([
      post('en', 'worktrees'),
      post('en', 'hidden', { draft: true, published: '2026-04-01' }),
      post('ja', 'worktrees', { published: '2026-03-03' }),
      post('fr', 'worktrees', { draft: true }),
    ]);
    assert.equal(errors.length, 0);
    assert.deepEqual(
      publicPosts(posts, 'en').map((item) => item.slug),
      ['worktrees'],
    );
    assert.deepEqual(
      publicPosts(posts, 'ja').map((item) => item.slug),
      ['worktrees'],
    );
    assert.deepEqual(
      publicPosts(posts, 'fr').map((item) => item.slug),
      [],
    );
    assert.deepEqual(blogLocalesForSlug(posts, 'worktrees'), ['en', 'ja']);
    assert.deepEqual(englishArticlePaths(posts), ['/blog/worktrees']);
  });

  it('uses the author date for an article and does not invent a locale lastmod', () => {
    const { posts } = collectBlogPosts([
      post('en', 'worktrees', { updated: '2026-03-09' }),
      post('ja', 'worktrees', { published: '2026-03-05' }),
    ]);
    assert.equal(blogLastmod(posts, 'en', '/blog/worktrees'), '2026-03-09');
    assert.equal(blogLastmod(posts, 'ja', '/blog/worktrees'), '2026-03-05');
    assert.equal(blogLastmod(posts, 'fr', '/blog/worktrees'), undefined);
    assert.equal(formatBlogDate('2026-03-09'), 'March 9, 2026');
    assert.equal(formatBlogDate('2026-11-11', 'zh-CN'), '2026年11月11日');
  });

  it('dates a locale index from that locale’s notes, not the English ones', () => {
    const { posts } = collectBlogPosts([
      post('en', 'worktrees', { updated: '2026-03-09' }),
      post('ja', 'worktrees', { published: '2026-03-05', updated: '2099-01-02' }),
    ]);
    assert.equal(blogLastmod(posts, 'ja', '/blog'), '2099-01-02');
    assert.notEqual(blogLastmod(posts, 'en', '/blog'), '2099-01-02');
    assert.notEqual(blogLastmod(posts, 'fr', '/blog'), '2099-01-02');
  });
});

describe('blog sitemap alternates', () => {
  it('lists a real translation and omits locales that have no file', () => {
    const sources = [
      post('en', 'worktrees'),
      post('ja', 'worktrees'),
      post('fr', 'worktrees', { draft: true }),
      post('en', 'hidden', { draft: true }),
      post('vi', 'hidden'),
    ];
    const { posts } = collectBlogPosts(sources);
    const catalog = buildCatalog([...publishedTranslationSources(posts), 'src/i18n/ui/ja.ts', 'src/i18n/ui/fr.ts']);
    assert.deepEqual(publishedTranslationSources(posts), ['src/content/blog/ja/worktrees.md']);
    assert.deepEqual(englishArticlePaths(posts), ['/blog/worktrees']);
    assert.deepEqual(blogLocalesForSlug(posts, 'hidden'), []);
    assert.deepEqual(translatedLocales(catalog, '/blog/worktrees'), blogLocalesForSlug(posts, 'worktrees'));
    const xml = sitemapXml({
      englishRoutes: ['/blog', ...englishArticlePaths(posts)],
      catalog,
      site: 'https://getcoppice.app',
      lastmod: (locale, englishPath) => blogLastmod(posts, locale, englishPath),
    });
    assert.match(xml, /<loc>https:\/\/getcoppice\.app\/blog\/worktrees<\/loc>\n    <lastmod>2026-03-02<\/lastmod>/);
    assert.match(xml, /<loc>https:\/\/getcoppice\.app\/ja\/blog\/worktrees<\/loc>/);
    assert.match(xml, /<loc>https:\/\/getcoppice\.app\/ja\/blog<\/loc>/);
    assert.match(xml, /<loc>https:\/\/getcoppice\.app\/fr\/blog<\/loc>/);
    assert.doesNotMatch(xml, /\/fr\/blog\/worktrees/);
    assert.doesNotMatch(xml, /blog\/worktrees<\/loc>[\s\S]*?hreflang="fr"/);
    assert.doesNotMatch(xml, /\/blog\/hidden/);
    assert.doesNotMatch(xml, /hreflang="vi"/);
    assert.match(xml, /hreflang="ja" href="https:\/\/getcoppice\.app\/ja\/blog\/worktrees"/);
    assert.match(xml, /hreflang="x-default" href="https:\/\/getcoppice\.app\/blog\/worktrees"/);
  });
});

describe('blog figures', () => {
  it('turns a titled image into a captioned figure and rejects a missing alt', () => {
    const tree = {
      type: 'root',
      children: [
        {
          type: 'element',
          tagName: 'p',
          properties: {},
          children: [
            {
              type: 'element',
              tagName: 'img',
              properties: { src: '/blog/worktrees/board.webp', alt: 'Demo board with two tickets', title: 'Demo board' },
              children: [],
            },
          ],
        },
      ],
    };
    assert.deepEqual(transformBlogImages(tree), []);
    const figure = tree.children[0] as { tagName: string; children: { tagName: string; properties?: { alt?: string; loading?: string }; children?: { value?: string }[] }[] };
    assert.equal(figure.tagName, 'figure');
    assert.equal(figure.children[0].properties?.alt, 'Demo board with two tickets');
    assert.equal(figure.children[0].properties?.loading, 'lazy');
    assert.equal(figure.children[1].children?.[0].value, 'Demo board');
    const missing = {
      type: 'root',
      children: [{ type: 'element', tagName: 'img', properties: { src: '/a.png', alt: '' }, children: [] }],
    };
    assert.match(transformBlogImages(missing).join('\n'), /alt text/);
  });

  it('rejects an h1 in the body and ignores non-blog files', () => {
    assert.match(blogHeadingErrors({ type: 'root', children: [{ type: 'heading', depth: 1, children: [] }] }).join('\n'), /only h1/);
    assert.equal(isBlogMarkdown({ path: '/repo/website/src/pages/docs/concepts.md' }), false);
    assert.equal(isBlogMarkdown({ path: '/repo/website/src/content/blog/en/worktrees.md' }), true);
  });
});
