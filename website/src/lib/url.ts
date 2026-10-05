/** Prefix a site path with Astro's configured base. External and hash-only URLs pass through. */
export function withBase(path: string): string {
  if (path === '#' || path.startsWith('#') || /^[a-z][a-z0-9+.-]*:/i.test(path)) {
    return path;
  }

  const base = import.meta.env.BASE_URL || '/';
  const hashAt = path.indexOf('#');
  const hash = hashAt >= 0 ? path.slice(hashAt) : '';
  const raw = hashAt >= 0 ? path.slice(0, hashAt) : path;
  const stripped = raw.replace(/^\//, '');
  const prefix = base.endsWith('/') ? base : `${base}/`;
  return `${prefix}${stripped}${hash}`;
}
