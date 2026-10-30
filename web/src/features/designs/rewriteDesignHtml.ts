import { apiFetch } from '../../lib/api';

// ponytail: quoted src/href plus url() only — srcset and unquoted attrs if a design needs them.

const ATTR_REF = /\b(src|href)\s*=\s*(["'])([^"']+)\2/gi;
const CSS_URL_REF = /url\(\s*(["']?)([^"')]+)\1\s*\)/gi;

const PREVIEW_CSP =
  "default-src 'none'; script-src 'unsafe-inline' 'unsafe-eval' blob:; style-src 'unsafe-inline' blob: data:; img-src blob: data:; font-src blob: data:; connect-src 'none'; frame-src 'none'";

function withPreviewCsp(html: string): string {
  return `<meta http-equiv="Content-Security-Policy" content="${PREVIEW_CSP}">${html}`;
}

function aborted(err: unknown, signal?: AbortSignal): boolean {
  return signal?.aborted === true || (err instanceof Error && err.name === 'AbortError');
}

function fresh(re: RegExp): RegExp {
  return new RegExp(re.source, re.flags);
}

function collectRefs(text: string): string[] {
  const seen = new Set<string>();
  const refs: string[] = [];
  const add = (raw: string) => {
    const ref = raw.trim();
    if (!ref || seen.has(ref)) return;
    seen.add(ref);
    refs.push(ref);
  };
  for (const match of text.matchAll(fresh(ATTR_REF))) add(match[3] ?? '');
  for (const match of text.matchAll(fresh(CSS_URL_REF))) add(match[2] ?? '');
  return refs;
}

function resolveDesignRef(fromFile: string, ref: string): string | null {
  const trimmed = ref.trim();
  if (
    !trimmed ||
    trimmed.startsWith('#') ||
    trimmed.startsWith('/') ||
    /^[a-z][a-z0-9+.-]*:/i.test(trimmed)
  ) {
    return null;
  }
  const pathOnly = trimmed.split(/[?#]/, 1)[0] ?? '';
  if (!pathOnly || pathOnly.split('/').includes('..')) return null;
  const slash = fromFile.lastIndexOf('/');
  const dir = slash === -1 ? '' : fromFile.slice(0, slash + 1);
  return `${dir}${pathOnly.replace(/^\.\//, '')}`;
}

function encodeDesignPath(path: string): string {
  return path.split('/').map((part) => encodeURIComponent(part)).join('/');
}

function mime(path: string): string {
  switch (path.split('.').pop()?.toLowerCase()) {
    case 'html':
      return 'text/html';
    case 'css':
      return 'text/css';
    case 'png':
      return 'image/png';
    case 'jpg':
    case 'jpeg':
      return 'image/jpeg';
    case 'gif':
      return 'image/gif';
    case 'webp':
      return 'image/webp';
    default:
      return 'application/octet-stream';
  }
}

function applyRewrites(text: string, urls: Map<string, string>): string {
  if (urls.size === 0) return text;
  const withAttrs = text.replace(
    fresh(ATTR_REF),
    (full, name: string, quote: string, ref: string) => {
      const next = urls.get(ref.trim());
      return next ? `${name}=${quote}${next}${quote}` : full;
    },
  );
  return withAttrs.replace(fresh(CSS_URL_REF), (full, quote: string, ref: string) => {
    const next = urls.get(ref.trim());
    return next ? `url(${quote}${next}${quote})` : full;
  });
}

async function fetchDesignFile(
  setId: string,
  path: string,
  signal?: AbortSignal,
): Promise<ArrayBuffer> {
  const res = await apiFetch(
    `/api/designs/${setId}/files/${encodeDesignPath(path)}`,
    signal ? { signal } : {},
  );
  if (!res.ok) throw new Error('missing');
  return res.arrayBuffer();
}

async function loadAsset(
  setId: string,
  filePath: string,
  track: (blob: Blob) => string,
  rewriteCss: boolean,
  signal?: AbortSignal,
): Promise<Blob> {
  const bytes = await fetchDesignFile(setId, filePath, signal);
  if (filePath.toLowerCase().endsWith('.html')) {
    return new Blob([withPreviewCsp(new TextDecoder().decode(bytes))], { type: 'text/html' });
  }
  if (!rewriteCss || !filePath.toLowerCase().endsWith('.css')) {
    return new Blob([bytes], { type: mime(filePath) });
  }
  const css = new TextDecoder().decode(bytes);
  const urls = new Map<string, string>();
  for (const ref of collectRefs(css)) {
    const resolved = resolveDesignRef(filePath, ref);
    if (!resolved) continue;
    try {
      const asset = await loadAsset(setId, resolved, track, false, signal);
      urls.set(ref, track(asset));
    } catch (err) {
      if (aborted(err, signal)) throw err;
      // A missing image leaves the authored url(); the page still opens.
    }
  }
  return new Blob([applyRewrites(css, urls)], { type: 'text/css' });
}

export async function rewriteDesignHtml(
  setId: string,
  entry: string,
  signal?: AbortSignal,
): Promise<{ url: string; revoke: () => void }> {
  if (!entry) throw new Error('missing');
  const created: string[] = [];
  const revoke = () => {
    if (typeof URL.revokeObjectURL !== 'function') return;
    for (const blobUrl of created) URL.revokeObjectURL(blobUrl);
  };
  const track = (blob: Blob) => {
    const url = URL.createObjectURL(blob);
    created.push(url);
    return url;
  };
  try {
    const html = new TextDecoder().decode(await fetchDesignFile(setId, entry, signal));
    const urls = new Map<string, string>();
    for (const ref of collectRefs(html)) {
      const resolved = resolveDesignRef(entry, ref);
      if (!resolved) continue;
      try {
        urls.set(ref, track(await loadAsset(setId, resolved, track, true, signal)));
      } catch (err) {
        if (aborted(err, signal)) throw err;
        // Missing stylesheet or image: leave the relative url and still show the page.
      }
    }
    const url = track(
      new Blob([withPreviewCsp(applyRewrites(html, urls))], { type: 'text/html' }),
    );
    return { url, revoke };
  } catch (err) {
    revoke();
    throw err;
  }
}
