/**
 * Detects repo file paths mentioned in ticket markdown (`src/lib.rs:42`,
 * `docs/shot.png`) so they can open a preview. Each preview kind owns its
 * extensions; add a kind here (and a renderer in FilePreviewModal) to support more.
 */
export type FilePreviewKind = 'image' | 'text';

export interface FileReference {
  path: string;
  line?: number;
  endLine?: number;
  kind: FilePreviewKind;
}

const IMAGE_EXTENSIONS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico', 'avif']);

/** Extension → highlight.js language (undefined: plain text, no highlighting). */
const TEXT_EXTENSIONS: Record<string, string | undefined> = {
  rs: 'rust',
  ts: 'typescript',
  tsx: 'typescript',
  mts: 'typescript',
  cts: 'typescript',
  js: 'javascript',
  jsx: 'javascript',
  mjs: 'javascript',
  cjs: 'javascript',
  json: 'json',
  jsonc: 'json',
  md: 'markdown',
  mdx: 'markdown',
  toml: 'ini',
  ini: 'ini',
  cfg: 'ini',
  conf: 'ini',
  yaml: 'yaml',
  yml: 'yaml',
  css: 'css',
  scss: 'css',
  html: 'xml',
  xml: 'xml',
  vue: 'xml',
  svelte: 'xml',
  astro: 'xml',
  sql: 'sql',
  sh: 'bash',
  bash: 'bash',
  zsh: 'bash',
  py: 'python',
  go: 'go',
  txt: undefined,
  log: undefined,
  csv: undefined,
  lock: undefined,
  rb: undefined,
  java: undefined,
  kt: undefined,
  swift: undefined,
  c: undefined,
  h: undefined,
  cpp: undefined,
  hpp: undefined,
  cs: undefined,
  proto: undefined,
  graphql: undefined,
};

const TEXT_FILENAMES: Record<string, string | undefined> = {
  Makefile: undefined,
  Dockerfile: undefined,
  Justfile: undefined,
  Procfile: undefined,
  LICENSE: undefined,
  '.gitignore': undefined,
  '.dockerignore': undefined,
  '.editorconfig': 'ini',
};

function basename(path: string): string {
  return path.slice(path.lastIndexOf('/') + 1);
}

function extension(path: string): string | null {
  const name = basename(path);
  const dot = name.lastIndexOf('.');
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : null;
}

export function filePreviewKind(path: string): FilePreviewKind | null {
  if (basename(path) in TEXT_FILENAMES) return 'text';
  const ext = extension(path);
  if (!ext) return null;
  if (IMAGE_EXTENSIONS.has(ext)) return 'image';
  if (ext in TEXT_EXTENSIONS) return 'text';
  return null;
}

export function highlightLanguage(path: string): string | undefined {
  const name = basename(path);
  if (name in TEXT_FILENAMES) return TEXT_FILENAMES[name];
  const ext = extension(path);
  return ext ? TEXT_EXTENSIONS[ext] : undefined;
}

const SEGMENT = String.raw`[\w@+-][\w.@+-]*`;
const REFERENCE_RE = new RegExp(
  String.raw`^(?:\./)?(${SEGMENT}(?:/${SEGMENT})*)(?::(\d+)(?:-(\d+))?|#L(\d+)(?:-L(\d+))?)?$`,
);

/** Parses `path`, `path:12`, `path:12-20` or `path#L12-L20`; null when not a previewable repo path. */
export function parseFileReference(raw: string): FileReference | null {
  const match = REFERENCE_RE.exec(raw.trim());
  if (!match) return null;
  const path = match[1];
  if (path.split('/').some((seg) => seg === '..' || seg === '.git')) return null;
  const kind = filePreviewKind(path);
  if (!kind) return null;
  const line = Number(match[2] ?? match[4]) || undefined;
  const endLine = Number(match[3] ?? match[5]) || undefined;
  return {
    path,
    kind,
    ...(line ? { line } : {}),
    ...(line && endLine && endLine >= line ? { endLine } : {}),
  };
}

// Plain prose only links paths with a directory ("src/a.rs"), so words like
// "Node.js" stay text. Inline code and links accept bare filenames too.
const PROSE_PATH_RE = new RegExp(
  String.raw`(?<![\w/.@+:-])(?:\./)?${SEGMENT}(?:/${SEGMENT})+(?::\d+(?:-\d+)?)?`,
  'g',
);

export interface ProseFileMatch {
  start: number;
  end: number;
  raw: string;
}

export function findProseFileReferences(text: string): ProseFileMatch[] {
  const matches: ProseFileMatch[] = [];
  for (const match of text.matchAll(PROSE_PATH_RE)) {
    // Sentence punctuation is not part of the path.
    const raw = match[0].replace(/[.]+$/, '');
    const start = match.index ?? 0;
    if (text[start + match[0].length] === '/') continue;
    if (parseFileReference(raw)) matches.push({ start, end: start + raw.length, raw });
  }
  return matches;
}

export function ticketFileUrl(ticketId: string, path: string): string {
  return `/api/tickets/${ticketId}/files/content?path=${encodeURIComponent(path)}`;
}

export function formatFileReference(ref: FileReference): string {
  if (!ref.line) return ref.path;
  return ref.endLine ? `${ref.path}:${ref.line}-${ref.endLine}` : `${ref.path}:${ref.line}`;
}
