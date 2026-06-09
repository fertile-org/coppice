/** OpenCode tool results often use a small XML envelope. */

export interface ParsedFileToolOutput {
  path?: string;
  type?: string;
  content: string;
}

const FILE_OUTPUT_RE =
  /<path>([\s\S]*?)<\/path>\s*<type>([\s\S]*?)<\/type>\s*<content>([\s\S]*?)<\/content>/i;

const SYSTEM_REMINDER_RE = /<system-reminder>[\s\S]*?<\/system-reminder>/gi;

export function stripSystemReminders(text: string): string {
  return text.replace(SYSTEM_REMINDER_RE, '').trim();
}

/** Remove OpenCode line-number prefixes (`1: `, `12: `) from read output. */
export function stripLineNumberPrefixes(text: string): string {
  return text
    .split('\n')
    .map((line) => line.replace(/^\s*\d+:\s?/, ''))
    .join('\n');
}

export function parseFileToolOutput(raw: string): ParsedFileToolOutput | null {
  const cleaned = stripSystemReminders(raw);
  const match = cleaned.match(FILE_OUTPUT_RE);
  if (!match) return null;

  const path = match[1].trim();
  const type = match[2].trim();
  let content = match[3].replace(/\n?\(End of file[^)]*\)\s*$/i, '').trim();
  content = stripLineNumberPrefixes(content).trim();

  return { path, type, content };
}

export function languageFromPath(filePath: string): string | undefined {
  const name = filePath.split('/').pop() ?? filePath;
  const dot = name.lastIndexOf('.');
  if (dot === -1) return undefined;
  const ext = name.slice(dot + 1).toLowerCase();

  const map: Record<string, string> = {
    md: 'markdown',
    markdown: 'markdown',
    json: 'json',
    js: 'javascript',
    jsx: 'javascript',
    ts: 'typescript',
    tsx: 'typescript',
    py: 'python',
    rs: 'rust',
    go: 'go',
    sh: 'bash',
    bash: 'bash',
    yml: 'yaml',
    yaml: 'yaml',
    toml: 'ini',
    sql: 'sql',
    html: 'xml',
    xml: 'xml',
    css: 'css',
    scss: 'scss',
  };

  return map[ext];
}

export function truncateText(text: string, maxLines = 40, maxChars = 12_000): string {
  const lines = text.split('\n');
  let out = lines.slice(0, maxLines).join('\n');
  if (lines.length > maxLines) {
    out += `\n\n… (${lines.length - maxLines} more lines)`;
  }
  if (out.length > maxChars) {
    out = `${out.slice(0, maxChars)}\n\n… (truncated)`;
  }
  return out;
}

/** Heuristic: fetched HTML / noisy web pages should not dump raw markup. */
export function isMostlyHtml(text: string): boolean {
  const sample = text.slice(0, 4000).toLowerCase();
  const tags = (sample.match(/<[a-z!/][^>]*>/g) ?? []).length;
  return tags > 8 || sample.includes('<!doctype html') || sample.includes('<html');
}

export function excerptWebContent(text: string): string {
  if (!isMostlyHtml(text)) {
    return truncateText(text, 48, 8_000);
  }

  const withoutScripts = text
    .replace(/<script[\s\S]*?<\/script>/gi, ' ')
    .replace(/<style[\s\S]*?<\/style>/gi, ' ');
  const plain = withoutScripts
    .replace(/<[^>]+>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();

  if (!plain) {
    return '(Fetched HTML page — preview hidden)';
  }

  return truncateText(plain, 24, 4_000);
}
