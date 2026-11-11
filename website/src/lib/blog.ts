import { lastCommitDate } from './lastmod.ts';
import { LOCALES, type LocalePath } from '../i18n/paths.ts';

export { BLOG_INDEX } from './blog-index.ts';

const SLUG = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
const DATE = /^(\d{4})-(\d{2})-(\d{2})$/;
const KEYS = new Set([
  'title',
  'metaTitle',
  'description',
  'answer',
  'published',
  'updated',
  'author',
  'authorType',
  'image',
  'draft',
  'keywords',
]);

const TITLE_MAX = 60;
const DESCRIPTION_MIN = 100;
const DESCRIPTION_MAX = 160;
const ANSWER_MIN = 40;
const ANSWER_MAX = 500;

const WIDE = /[\u1100-\u115f\u2e80-\ua4cf\uac00-\ud7a3\uf900-\ufaff\ufe30-\ufe4f\uff00-\uff60\uffe0-\uffe6]/u;

export type BlogAuthorType = 'Person' | 'Organization';

export type BlogPost = {
  file: string;
  locale: LocalePath;
  slug: string;
  englishPath: string;
  title: string;
  metaTitle?: string;
  description: string;
  answer: string;
  published: string;
  updated?: string;
  author: string;
  authorType: BlogAuthorType;
  image?: string;
  draft: boolean;
  keywords: string[];
};

export type BlogSource = { file: string; data: unknown };

/** Display width used by `check:seo`: CJK and fullwidth characters count as two. */
export function displayWidth(text: string): number {
  let total = 0;
  for (const char of text) total += WIDE.test(char) ? 2 : 1;
  return total;
}

/** `<title>` after `Base.astro` appends ` · Coppice` when the text does not already name it. */
export function renderedTitle(title: string, metaTitle?: string): string {
  const chosen = (metaTitle ?? title).trim();
  return chosen.includes('Coppice') ? chosen : `${chosen} · Coppice`;
}

/** `locale` is a BCP 47 code (`en`, `zh-CN`). The index and a translated article pass the page locale. */
export function formatBlogDate(iso: string, locale = 'en'): string {
  const match = DATE.exec(iso);
  if (!match) return iso;
  const date = new Date(Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3])));
  return new Intl.DateTimeFormat(locale, { dateStyle: 'long', timeZone: 'UTC' }).format(date);
}

/** Repo-relative `src/content/blog/...` path, or the original string when it is not one. */
export function blogRepoPath(file: string): string {
  const path = file.replace(/\\/g, '/');
  const marker = '/src/content/blog/';
  const at = path.indexOf(marker);
  if (at >= 0) return `src/content/blog/${path.slice(at + marker.length)}`;
  if (path.startsWith('../content/blog/')) return `src/${path.slice(3)}`;
  if (path.startsWith('content/blog/')) return `src/${path}`;
  return path;
}

function localeOf(path: string): LocalePath | null {
  const found = LOCALES.find((locale) => locale.path === path);
  return found ? (found.path as LocalePath) : null;
}

function slugFromRelative(relative: string): string | null {
  let slug = relative.replace(/\.(md|mdx)$/i, '');
  if (slug.endsWith('/index')) slug = slug.slice(0, -'/index'.length);
  if (!slug || slug.includes('/') || slug === 'index' || !SLUG.test(slug)) return null;
  return slug;
}

export function blogSourceIdentity(file: string): { locale: LocalePath; slug: string; file: string } | null {
  const repo = blogRepoPath(file);
  const match = repo.match(/^src\/content\/blog\/([^/]+)\/(.+)$/);
  if (!match) return null;
  const locale = localeOf(match[1]);
  const slug = slugFromRelative(match[2]);
  if (!locale || !slug) return null;
  return { locale, slug, file: repo };
}

function isRealDate(value: string): boolean {
  const match = DATE.exec(value);
  if (!match) return false;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const date = new Date(Date.UTC(year, month - 1, day));
  return date.getUTCFullYear() === year && date.getUTCMonth() === month - 1 && date.getUTCDate() === day;
}

function formatUtcDate(date: Date): string {
  const month = String(date.getUTCMonth() + 1).padStart(2, '0');
  const day = String(date.getUTCDate()).padStart(2, '0');
  return `${date.getUTCFullYear()}-${month}-${day}`;
}

function readDate(value: unknown, field: string, errors: string[]): string | undefined {
  if (typeof value === 'string') {
    if (!isRealDate(value)) errors.push(`${field} must be a real YYYY-MM-DD date`);
    else return value;
    return undefined;
  }
  if (value instanceof Date && !Number.isNaN(value.getTime())) {
    const midnight =
      value.getUTCHours() === 0 &&
      value.getUTCMinutes() === 0 &&
      value.getUTCSeconds() === 0 &&
      value.getUTCMilliseconds() === 0;
    if (!midnight) {
      errors.push(`${field} must be a calendar date, quoted as YYYY-MM-DD`);
      return undefined;
    }
    const formatted = formatUtcDate(value);
    if (!isRealDate(formatted)) errors.push(`${field} must be a real YYYY-MM-DD date`);
    else return formatted;
    return undefined;
  }
  errors.push(`${field} must be a real YYYY-MM-DD date`);
  return undefined;
}

function readText(value: unknown, field: string, errors: string[]): string | undefined {
  if (typeof value !== 'string' || !value.trim()) {
    errors.push(`${field} is required`);
    return undefined;
  }
  return value.trim();
}

export function validateBlogSource(file: string, data: unknown): { post?: BlogPost; errors: string[] } {
  const errors: string[] = [];
  const identity = blogSourceIdentity(file);
  if (!identity) {
    return { errors: [`${blogRepoPath(file)}: path must be src/content/blog/<locale>/<slug>.md`] };
  }
  if (!data || typeof data !== 'object' || Array.isArray(data)) {
    return { errors: [`${identity.file}: frontmatter must be a mapping`] };
  }
  const record = data as Record<string, unknown>;
  for (const key of Object.keys(record)) {
    if (!KEYS.has(key)) errors.push(`unknown frontmatter key ${key}`);
  }
  const title = readText(record.title, 'title', errors);
  const description = readText(record.description, 'description', errors);
  const answer = readText(record.answer, 'answer', errors);
  const author = readText(record.author, 'author', errors);
  const published = readDate(record.published, 'published', errors);
  let updated: string | undefined;
  if (record.updated !== undefined) updated = readDate(record.updated, 'updated', errors);
  if (published && updated && updated < published) errors.push('updated is earlier than published');

  let metaTitle: string | undefined;
  if (record.metaTitle !== undefined) {
    metaTitle = readText(record.metaTitle, 'metaTitle', errors);
  }
  if (title && displayWidth(renderedTitle(title, metaTitle)) > TITLE_MAX) {
    errors.push(`title is wider than ${TITLE_MAX} once Coppice is added`);
  }
  if (description && (displayWidth(description) < DESCRIPTION_MIN || displayWidth(description) > DESCRIPTION_MAX)) {
    errors.push(`description width must be ${DESCRIPTION_MIN}–${DESCRIPTION_MAX}`);
  }
  if (answer && (answer.length < ANSWER_MIN || answer.length > ANSWER_MAX)) {
    errors.push(`answer must be ${ANSWER_MIN}–${ANSWER_MAX} characters`);
  }
  if (author && author.length > 80) errors.push('author is longer than 80 characters');

  let authorType: BlogAuthorType = 'Person';
  if (record.authorType !== undefined) {
    if (record.authorType !== 'Person' && record.authorType !== 'Organization') {
      errors.push('authorType must be Person or Organization');
    } else {
      authorType = record.authorType;
    }
  }

  let image: string | undefined;
  if (record.image !== undefined) {
    if (typeof record.image !== 'string' || !record.image.trim()) errors.push('image must be a path or URL');
    else {
      image = record.image.trim();
      const sitePath = image.startsWith('/') && !image.startsWith('//');
      const remote = image.startsWith('https://');
      if ((!sitePath && !remote) || image.includes('..')) {
        errors.push('image must be a site path starting with / or an https URL');
      }
    }
  }

  let draft = false;
  if (record.draft !== undefined) {
    if (typeof record.draft !== 'boolean') errors.push('draft must be true or false');
    else draft = record.draft;
  }

  const keywords: string[] = [];
  if (record.keywords !== undefined) {
    if (!Array.isArray(record.keywords) || record.keywords.some((item) => typeof item !== 'string' || !item.trim())) {
      errors.push('keywords must be a list of strings');
    } else if (record.keywords.length > 8) {
      errors.push('keywords has more than 8 entries');
    } else {
      keywords.push(...record.keywords.map((item) => item.trim()));
    }
  }

  if (errors.length || !title || !description || !answer || !author || !published) {
    return { errors: errors.map((error) => `${identity.file}: ${error}`) };
  }
  return {
    errors: [],
    post: {
      file: identity.file,
      locale: identity.locale,
      slug: identity.slug,
      englishPath: `/blog/${identity.slug}`,
      title,
      ...(metaTitle ? { metaTitle } : {}),
      description,
      answer,
      published,
      ...(updated ? { updated } : {}),
      author,
      authorType,
      ...(image ? { image } : {}),
      draft,
      keywords,
    },
  };
}

export function collectBlogPosts(sources: readonly BlogSource[]): { posts: BlogPost[]; errors: string[] } {
  const errors: string[] = [];
  const posts: BlogPost[] = [];
  const seen = new Set<string>();
  for (const source of sources) {
    const result = validateBlogSource(source.file, source.data);
    errors.push(...result.errors);
    if (!result.post) continue;
    const key = `${result.post.locale}/${result.post.slug}`;
    if (seen.has(key)) errors.push(`${result.post.file}: duplicate ${key}`);
    seen.add(key);
    posts.push(result.post);
  }
  const english = new Set(posts.filter((post) => post.locale === 'en').map((post) => post.slug));
  for (const post of posts) {
    if (post.locale !== 'en' && !english.has(post.slug)) {
      errors.push(`${post.file}: translation has no English article at the same slug`);
    }
  }
  return { posts, errors };
}

export function loadBlogPosts(sources: readonly BlogSource[]): BlogPost[] {
  const { posts, errors } = collectBlogPosts(sources);
  if (errors.length) throw new Error(errors.join('\n'));
  return posts;
}

export function comparePosts(a: BlogPost, b: BlogPost): number {
  if (a.published !== b.published) return a.published < b.published ? 1 : -1;
  return a.slug.localeCompare(b.slug);
}

export function publicPosts(posts: readonly BlogPost[], locale?: LocalePath): BlogPost[] {
  return posts
    .filter((post) => !post.draft && (locale === undefined || post.locale === locale))
    .slice()
    .sort(comparePosts);
}

export function englishArticlePaths(posts: readonly BlogPost[]): string[] {
  return publicPosts(posts, 'en')
    .map((post) => post.englishPath)
    .sort((a, b) => a.localeCompare(b));
}

/**
 * Translation files that belong in hreflang and the sitemap.
 * A draft translation is omitted. A translation of a draft English article is omitted too.
 */
export function publishedTranslationSources(posts: readonly BlogPost[]): string[] {
  const english = new Set(publicPosts(posts, 'en').map((post) => post.slug));
  return posts
    .filter((post) => post.locale !== 'en' && !post.draft && english.has(post.slug))
    .map((post) => post.file)
    .sort((a, b) => a.localeCompare(b));
}

/** Locales with a real, non-draft article at this slug. English is included when that file is public. */
export function blogLocalesForSlug(posts: readonly BlogPost[], slug: string): LocalePath[] {
  const have = new Set(posts.filter((post) => post.slug === slug && !post.draft).map((post) => post.locale));
  if (!have.has('en')) return [];
  return LOCALES.map((locale) => locale.path as LocalePath).filter((locale) => have.has(locale));
}

function latest(dates: readonly (string | undefined)[]): string | undefined {
  return dates.filter((date): date is string => Boolean(date)).sort().at(-1);
}

/**
 * Sitemap `lastmod`. Articles use the author-supplied `updated` or `published` date.
 * Each locale index uses that locale's notes, then the later of those dates and the
 * index page's git commit, when git has one. A locale with no notes does not borrow
 * the English dates.
 */
export function blogLastmod(posts: readonly BlogPost[], locale: string, englishPath: string): string | undefined {
  if (englishPath === '/blog') {
    const found = LOCALES.find((item) => item.path === locale);
    const authored = found
      ? publicPosts(posts, found.path as LocalePath).map((post) => post.updated ?? post.published)
      : [];
    return latest([
      ...authored,
      lastCommitDate(['src/pages/blog/index.astro', 'src/components/BlogIndex.astro', `src/i18n/ui/${locale}.ts`]),
    ]);
  }
  if (!englishPath.startsWith('/blog/')) return undefined;
  const slug = englishPath.slice('/blog/'.length);
  const post = posts.find((item) => item.slug === slug && item.locale === locale && !item.draft);
  if (!post) return undefined;
  return post.updated ?? post.published;
}
