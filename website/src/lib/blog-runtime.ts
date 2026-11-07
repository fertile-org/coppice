import { loadBlogPosts, publicPosts, type BlogPost } from './blog';

type BlogModule = {
  Content: (props: Record<string, unknown>) => unknown;
  frontmatter?: Record<string, unknown>;
};

const modules = import.meta.glob('../content/blog/**/*.{md,mdx}', { eager: true }) as Record<string, BlogModule>;

export type BlogEntry = { post: BlogPost; Content: BlogModule['Content'] };

function moduleFor(file: string): BlogModule {
  const suffix = file.replace(/^src\//, '');
  const key = Object.keys(modules).find((item) => item.replace(/\\/g, '/').endsWith(suffix));
  const mod = key ? modules[key] : undefined;
  if (!mod?.Content) throw new Error(`blog module missing for ${file}`);
  return mod;
}

let cache: BlogEntry[] | undefined;

/** Every article file. Invalid frontmatter fails the build. */
export function blogEntries(): BlogEntry[] {
  if (cache) return cache;
  const posts = loadBlogPosts(
    Object.entries(modules).map(([file, mod]) => ({ file, data: mod.frontmatter ?? {} })),
  );
  cache = posts.map((post) => ({ post, Content: moduleFor(post.file).Content }));
  return cache;
}

export function englishEntries(): BlogEntry[] {
  const order = new Map(publicPosts(blogEntries().map((entry) => entry.post), 'en').map((post, index) => [post.file, index]));
  return blogEntries()
    .filter((entry) => order.has(entry.post.file))
    .sort((a, b) => (order.get(a.post.file) ?? 0) - (order.get(b.post.file) ?? 0));
}

export function entryFor(locale: string, slug: string): BlogEntry | null {
  return blogEntries().find((entry) => entry.post.locale === locale && entry.post.slug === slug && !entry.post.draft) ?? null;
}
