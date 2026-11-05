import { apiFetch } from '../../lib/api';

// ponytail: quoted src/href plus url() only — srcset and unquoted attrs if a design needs them.
// shortcut: a stylesheet that contains </style> closes the inlined tag early, escape it if a design ships that.

const ATTR_REF = /\b(src|href)\s*=\s*(["'])([^"']+)\2/gi;
const CSS_URL_REF = /url\(\s*(["']?)([^"')]+)\1\s*\)/gi;
const LINK_TAG = /<link\b[^>]*>/gi;

const PREVIEW_CSP =
  "default-src 'none'; script-src 'unsafe-inline' 'unsafe-eval' blob:; style-src 'unsafe-inline' data:; img-src data:; font-src data:; connect-src 'none'; frame-src 'none'";

function withPreviewCsp(html: string): string {
  const meta = `<meta http-equiv="Content-Security-Policy" content="${PREVIEW_CSP}">`;
  const head = /<head\b[^>]*>/i.exec(html);
  if (head?.index !== undefined) {
    const at = head.index + head[0].length;
    return html.slice(0, at) + meta + html.slice(at);
  }
  const doctype = /^\s*<!doctype\b[^>]*>/i.exec(html);
  if (doctype?.index !== undefined) {
    const at = doctype.index + doctype[0].length;
    return html.slice(0, at) + meta + html.slice(at);
  }
  return meta + html;
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
    case 'svg':
      return 'image/svg+xml';
    case 'woff':
      return 'font/woff';
    case 'woff2':
      return 'font/woff2';
    default:
      return 'application/octet-stream';
  }
}

function quotedAttr(tag: string, name: string): string | null {
  const match = new RegExp(`\\b${name}\\s*=\\s*(["'])([^"']*)\\1`, 'i').exec(tag);
  return match ? (match[2] ?? null) : null;
}

function stylesheetHref(tag: string): string | null {
  const rel = quotedAttr(tag, 'rel');
  if (!rel?.toLowerCase().split(/\s+/).includes('stylesheet')) return null;
  return quotedAttr(tag, 'href');
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

function blobToDataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result !== 'string') {
        reject(new Error('read'));
        return;
      }
      resolve(reader.result);
    };
    reader.onerror = () => reject(reader.error ?? new Error('read'));
    reader.readAsDataURL(blob);
  });
}

async function rewriteRefsToData(
  setId: string,
  fromFile: string,
  text: string,
  signal: AbortSignal | undefined,
  stack: string[],
): Promise<string> {
  const urls = new Map<string, string>();
  for (const ref of collectRefs(text)) {
    const resolved = resolveDesignRef(fromFile, ref);
    if (!resolved) continue;
    try {
      urls.set(ref, await loadDataUrl(setId, resolved, signal, stack));
    } catch (err) {
      if (aborted(err, signal)) throw err;
      // A missing image leaves the authored url; the page still opens.
    }
  }
  return applyRewrites(text, urls);
}

async function loadDataUrl(
  setId: string,
  filePath: string,
  signal: AbortSignal | undefined,
  stack: string[],
): Promise<string> {
  if (stack.includes(filePath)) throw new Error('cycle');
  const bytes = new Uint8Array(await fetchDesignFile(setId, filePath, signal));
  const lower = filePath.toLowerCase();
  if (lower.endsWith('.css')) {
    const css = await rewriteRefsToData(
      setId,
      filePath,
      new TextDecoder().decode(bytes),
      signal,
      [...stack, filePath],
    );
    return blobToDataUrl(new Blob([css], { type: 'text/css' }));
  }
  if (lower.endsWith('.html')) {
    return blobToDataUrl(
      new Blob([withPreviewCsp(new TextDecoder().decode(bytes))], { type: 'text/html' }),
    );
  }
  return blobToDataUrl(new Blob([bytes], { type: mime(filePath) }));
}

async function loadCssText(
  setId: string,
  filePath: string,
  signal?: AbortSignal,
): Promise<string> {
  const bytes = await fetchDesignFile(setId, filePath, signal);
  return rewriteRefsToData(
    setId,
    filePath,
    new TextDecoder().decode(bytes),
    signal,
    [filePath],
  );
}

async function inlineStylesheets(
  setId: string,
  entry: string,
  html: string,
  signal?: AbortSignal,
): Promise<string> {
  const matches = [...html.matchAll(fresh(LINK_TAG))];
  if (matches.length === 0) return html;
  let out = '';
  let cursor = 0;
  for (const match of matches) {
    const tag = match[0];
    const index = match.index ?? 0;
    out += html.slice(cursor, index);
    cursor = index + tag.length;
    const href = stylesheetHref(tag);
    const resolved = href ? resolveDesignRef(entry, href) : null;
    if (!resolved?.toLowerCase().endsWith('.css')) {
      out += tag;
      continue;
    }
    try {
      out += `<style>${await loadCssText(setId, resolved, signal)}</style>`;
    } catch (err) {
      if (aborted(err, signal)) throw err;
      out += tag;
    }
  }
  return out + html.slice(cursor);
}

export async function rewriteDesignHtml(
  setId: string,
  entry: string,
  signal?: AbortSignal,
): Promise<{ url: string; revoke: () => void }> {
  if (!entry) throw new Error('missing');
  let url: string | null = null;
  const revoke = () => {
    if (!url || typeof URL.revokeObjectURL !== 'function') return;
    URL.revokeObjectURL(url);
  };
  try {
    const html = new TextDecoder().decode(await fetchDesignFile(setId, entry, signal));
    const inlined = await inlineStylesheets(setId, entry, html, signal);
    const rewritten = await rewriteRefsToData(setId, entry, inlined, signal, []);
    url = URL.createObjectURL(
      new Blob([withPreviewCsp(rewritten)], { type: 'text/html' }),
    );
    return { url, revoke };
  } catch (err) {
    revoke();
    throw err;
  }
}
