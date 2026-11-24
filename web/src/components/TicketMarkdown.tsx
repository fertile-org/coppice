import { Children, isValidElement, type ReactNode } from 'react';
import type { Components } from 'react-markdown';
import ReactMarkdown, { defaultUrlTransform } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import {
  parseFileReference,
  resolveRelativePath,
  ticketFileUrl,
} from '../features/files/fileReference';
import { FileReferenceLink } from '../features/files/FileReferenceLink';
import {
  FILE_REFERENCE_URL_PREFIX,
  remarkFileReferences,
} from '../features/files/remarkFileReferences';
import { MermaidDiagram } from './MermaidDiagram';

export type MarkdownVariant = 'comment' | 'document';

export type FrontmatterField = { key: string; value: string };

/**
 * Pull a leading YAML frontmatter block when line 1 is `---` and a closing
 * `---` follows. Unclosed openers stay in the body so thematic breaks still render.
 */
export function splitMarkdownFrontmatter(text: string): {
  fields: FrontmatterField[] | null;
  body: string;
} {
  const lines = text.replace(/^\uFEFF/, '').split('\n');
  if (lines[0]?.trim() !== '---') {
    return { fields: null, body: text };
  }
  let closeIdx = -1;
  for (let i = 1; i < lines.length; i += 1) {
    if (lines[i]?.trim() === '---') {
      closeIdx = i;
      break;
    }
  }
  if (closeIdx < 0) {
    return { fields: null, body: text };
  }

  const fields: FrontmatterField[] = [];
  for (const line of lines.slice(1, closeIdx)) {
    if (!line.trim()) continue;
    const match = /^([A-Za-z0-9_-]+)\s*:\s*(.*)$/.exec(line);
    if (!match) continue;
    let value = (match[2] ?? '').trim();
    if (
      (value.startsWith('"') && value.endsWith('"')) ||
      (value.startsWith("'") && value.endsWith("'"))
    ) {
      value = value.slice(1, -1);
    }
    fields.push({ key: match[1] ?? '', value });
  }

  const body = lines.slice(closeIdx + 1).join('\n');
  return { fields, body };
}

function fenceLanguage(className: string | undefined): string | undefined {
  return /language-([\w-]+)/.exec(className ?? '')?.[1]?.toLowerCase();
}

function codeText(children: ReactNode): string {
  if (Array.isArray(children)) {
    return children
      .map((part) =>
        typeof part === 'string' || typeof part === 'number' ? String(part) : '',
      )
      .join('')
      .replace(/\n$/, '');
  }
  return String(children ?? '').replace(/\n$/, '');
}

function mermaidFenceSource(children: ReactNode): string | null {
  for (const node of Children.toArray(children)) {
    if (!isValidElement(node)) continue;
    const props = node.props as { className?: string; children?: ReactNode };
    if (fenceLanguage(props.className) !== 'mermaid') continue;
    return codeText(props.children).trim();
  }
  return null;
}

/** Strip an optional `:line` / `#Lline` suffix so a relative path can be resolved. */
function splitHrefPath(href: string): { path: string; suffix: string } {
  const match =
    /^(.*?)(?::(\d+)(?:-(\d+))?|#L(\d+)(?:-L(\d+))?)?$/.exec(href.trim()) ?? undefined;
  if (!match?.[1]) return { path: href.trim(), suffix: '' };
  return { path: match[1], suffix: href.trim().slice(match[1].length) };
}

/** A link to a repo file: one found in prose, or a relative markdown link. */
function linkedFile(href: string | undefined, basePath?: string) {
  if (!href) return null;
  if (href.startsWith(FILE_REFERENCE_URL_PREFIX)) {
    return parseFileReference(decodeURIComponent(href.slice(FILE_REFERENCE_URL_PREFIX.length)));
  }
  // Absolute URLs, anchors and site paths keep their normal link behavior.
  if (/^[a-z][\w+.-]*:|^[#/?]/i.test(href)) return null;
  if (basePath) {
    const { path, suffix } = splitHrefPath(href);
    const resolved = resolveRelativePath(basePath, path);
    if (!resolved) return null;
    return parseFileReference(resolved + suffix);
  }
  return parseFileReference(href);
}

function urlTransform(url: string): string {
  return url.startsWith(FILE_REFERENCE_URL_PREFIX) ? url : defaultUrlTransform(url);
}

function FrontmatterProperties({ fields }: { fields: FrontmatterField[] }) {
  if (fields.length === 0) return null;
  return (
    <div
      className="mb-7 border-y border-border py-3"
      data-testid="markdown-frontmatter"
    >
      <div className="mb-1.5 text-xs uppercase tracking-[0.06em] text-text-muted">
        Properties
      </div>
      {fields.map(({ key, value }) => (
        <div key={key} className="flex gap-3 py-0.5 text-sm">
          <span className="w-[110px] shrink-0 font-normal text-text-secondary">{key}</span>
          <span className="min-w-0 text-text-primary">{value}</span>
        </div>
      ))}
    </div>
  );
}

function markdownComponents(options?: {
  mermaidAsCode?: boolean;
  basePath?: string;
  ticketId?: string;
  variant?: MarkdownVariant;
}): Components {
  const document = options?.variant === 'document';

  const comps: Components = {
    h1: ({ children }) => (
      <h1
        className={
          document
            ? 'mb-5 mt-0 font-display text-[1.875rem] font-bold leading-tight text-text-primary first:mt-0'
            : 'mb-3 mt-5 font-display text-lg font-semibold text-text-primary first:mt-0'
        }
      >
        {children}
      </h1>
    ),
    h2: ({ children }) => (
      <h2
        className={
          document
            ? 'mb-3 mt-9 font-display text-[1.4375rem] font-bold leading-snug text-accent-hover first:mt-0'
            : 'mb-2 mt-5 font-display text-base font-semibold text-text-primary first:mt-0'
        }
      >
        {children}
      </h2>
    ),
    h3: ({ children }) => (
      <h3
        className={
          document
            ? 'mb-2.5 mt-7 font-display text-[1.1875rem] font-bold leading-snug text-accent first:mt-0'
            : 'mb-2 mt-4 font-body text-sm font-semibold text-text-primary first:mt-0'
        }
      >
        {children}
      </h3>
    ),
    p: ({ children }) => (
      <p
        className={
          document
            ? 'mb-4 font-body text-base leading-[1.75] text-text-secondary last:mb-0'
            : 'mb-3 font-body text-sm leading-relaxed text-text-primary last:mb-0'
        }
      >
        {children}
      </p>
    ),
    ul: ({ children }) => (
      <ul
        className={
          document
            ? 'mb-4 list-disc space-y-1.5 pl-7 font-body text-base leading-[1.75] text-text-secondary marker:text-accent'
            : 'mb-3 list-disc space-y-1.5 pl-5 font-body text-sm leading-relaxed text-text-primary'
        }
      >
        {children}
      </ul>
    ),
    ol: ({ children }) => (
      <ol
        className={
          document
            ? 'mb-4 list-decimal space-y-1.5 pl-7 font-body text-base leading-[1.75] text-text-secondary marker:text-accent'
            : 'mb-3 list-decimal space-y-1.5 pl-5 font-body text-sm leading-relaxed text-text-primary'
        }
      >
        {children}
      </ol>
    ),
    li: ({ children }) => (
      <li className={document ? 'leading-[1.75]' : 'leading-relaxed'}>{children}</li>
    ),
    strong: ({ children }) => (
      <strong className="font-semibold text-text-primary">{children}</strong>
    ),
    a: ({ href, children }) => {
      const file = linkedFile(href, options?.basePath);
      if (file) return <FileReferenceLink file={file}>{children}</FileReferenceLink>;
      if (href?.startsWith(FILE_REFERENCE_URL_PREFIX)) return <>{children}</>;
      const external = href != null && /^https?:\/\//i.test(href);
      return (
        <a
          href={href}
          className={
            document
              ? 'text-accent underline decoration-accent/40 underline-offset-[3px] hover:decoration-accent'
              : 'text-accent underline-offset-2 hover:underline'
          }
          {...(external ? { target: '_blank', rel: 'noreferrer' } : {})}
        >
          {children}
        </a>
      );
    },
    img: ({ src, alt }) => {
      if (!options?.basePath || !options.ticketId || !src) return null;
      if (/^[a-z][\w+.-]*:|^[#/?]/i.test(src)) {
        if (/^https?:\/\//i.test(src)) {
          return (
            <img
              src={src}
              alt={alt ?? ''}
              className="my-3 max-h-[70vh] max-w-full rounded-md object-contain"
            />
          );
        }
        return null;
      }
      const resolved = resolveRelativePath(options.basePath, src);
      if (!resolved) return null;
      return (
        <img
          src={ticketFileUrl(options.ticketId, resolved)}
          alt={alt || resolved}
          className="my-3 max-h-[70vh] max-w-full rounded-md object-contain"
        />
      );
    },
    code: ({ className, children }) => {
      const inline = (
        <code
          className={
            document
              ? 'rounded bg-paper-200 px-1.5 font-mono text-[0.85em] text-warning'
              : 'rounded bg-paper-200 px-1 font-mono text-[0.9em]'
          }
        >
          {children}
        </code>
      );
      // Only inline spans: fenced blocks carry a language class or a trailing newline.
      if (className || typeof children !== 'string' || children.includes('\n')) return inline;
      const file = parseFileReference(children);
      return file ? <FileReferenceLink file={file}>{inline}</FileReferenceLink> : inline;
    },
    pre: ({ children }) => {
      const diagram = options?.mermaidAsCode ? null : mermaidFenceSource(children);
      if (diagram !== null) return <MermaidDiagram source={diagram} />;
      return (
        <pre
          className={
            document
              ? 'mb-4 overflow-x-auto rounded-lg border border-border bg-paper-100 p-4 font-mono text-[13.5px] leading-relaxed text-text-secondary [&_code]:bg-transparent [&_code]:p-0 [&_code]:text-inherit'
              : 'mb-3 overflow-x-auto rounded-md border border-border bg-paper-200 p-3 font-mono text-xs leading-relaxed'
          }
        >
          {children}
        </pre>
      );
    },
    blockquote: ({ children }) => (
      <blockquote
        className={
          document
            ? 'mb-4 rounded-r-md border-l-[3px] border-accent bg-accent-muted px-[18px] py-2.5 text-text-secondary'
            : 'mb-3 border-l-2 border-border pl-3 text-text-secondary'
        }
      >
        {children}
      </blockquote>
    ),
    table: ({ children }) => (
      <div className={document ? 'mb-[18px] overflow-x-auto' : 'mb-4 overflow-x-auto'}>
        <table
          className={
            document
              ? 'min-w-full border-collapse font-body text-[15px]'
              : 'min-w-full border-collapse font-body text-sm'
          }
        >
          {children}
        </table>
      </div>
    ),
    thead: ({ children }) => (
      <thead className={document ? undefined : 'bg-paper-200'}>{children}</thead>
    ),
    th: ({ children }) => (
      <th
        className={
          document
            ? 'border-b-2 border-border-strong px-3.5 py-2 text-left font-semibold text-text-primary'
            : 'border border-border px-3 py-2 text-left font-semibold text-text-primary'
        }
      >
        {children}
      </th>
    ),
    td: ({ children }) => (
      <td
        className={
          document
            ? 'border-b border-border px-3.5 py-2 align-top text-text-secondary'
            : 'border border-border px-3 py-2 align-top text-text-primary'
        }
      >
        {children}
      </td>
    ),
  };

  if (document) {
    comps.h4 = ({ children }) => (
      <h4 className="mb-2 mt-6 font-body text-base font-semibold text-text-primary first:mt-0">
        {children}
      </h4>
    );
    comps.h5 = ({ children }) => (
      <h5 className="mb-2 mt-5 font-body text-sm font-semibold text-text-primary first:mt-0">
        {children}
      </h5>
    );
    comps.h6 = ({ children }) => (
      <h6 className="mb-2 mt-4 font-body text-sm font-semibold text-text-secondary first:mt-0">
        {children}
      </h6>
    );
    comps.hr = () => <hr className="my-8 border-border" />;
    comps.del = ({ children }) => (
      <del className="text-text-muted line-through">{children}</del>
    );
    comps.input = (props) => {
      if (props.type === 'checkbox') {
        return (
          <input
            type="checkbox"
            className="mr-2 align-middle accent-[var(--color-accent)]"
            defaultChecked={Boolean(props.checked)}
            disabled
            readOnly
          />
        );
      }
      return <input {...props} />;
    };
  }

  return comps;
}

const components = markdownComponents();
const mermaidAsCodeComponents = markdownComponents({ mermaidAsCode: true });

const AGENT_REQUESTS_MARKER_PREFIX = '<!-- coppice-agent-requests: ';

function isAgentRequestsMarkerLine(line: string): boolean {
  const trimmed = line.trim();
  return (
    trimmed.startsWith(AGENT_REQUESTS_MARKER_PREFIX) && trimmed.endsWith(' -->')
  );
}

export function normalizeCommentMarkdown(text: string): string {
  // Drop durable Coppice metadata markers from display only (storage keeps them).
  const withoutMarkers = text
    .split('\n')
    .filter((line) => !isAgentRequestsMarkerLine(line))
    .join('\n')
    .replace(/\n{3,}/g, '\n\n')
    .replace(/\n+$/g, '');

  // Ensure markdown block sections (e.g. **Tests run:**) start on their own paragraph.
  return withoutMarkers.replace(/([^\n])\n(\*\*[^*]+:\*\*)/g, '$1\n\n$2');
}

export function TicketMarkdown({
  children,
  mermaidAsCode = false,
  basePath,
  ticketId,
  variant = 'comment',
}: {
  children: string;
  /** Keep mermaid fences as code so a streaming source does not remount a diagram. */
  mermaidAsCode?: boolean;
  /** When set (file preview), resolve relative link/image hrefs against this file. */
  basePath?: string;
  /** Ticket whose worktree serves preview images when `basePath` is set. */
  ticketId?: string;
  /** `document` = file preview reading view; default keeps comment/chat/plan look. */
  variant?: MarkdownVariant;
}) {
  const trimmed = children.trim();
  if (!trimmed) return null;

  if (variant === 'document') {
    const { fields, body } = splitMarkdownFrontmatter(trimmed);
    const text = normalizeCommentMarkdown(body.trim());
    const comps = markdownComponents({
      mermaidAsCode,
      basePath,
      ticketId,
      variant: 'document',
    });

    return (
      <div className="mx-auto max-w-[720px] px-11 pb-14 pt-9 font-body text-base leading-[1.75] text-text-secondary">
        {fields ? <FrontmatterProperties fields={fields} /> : null}
        {text ? (
          <ReactMarkdown
            remarkPlugins={[remarkGfm, remarkFileReferences]}
            components={comps}
            urlTransform={urlTransform}
          >
            {text}
          </ReactMarkdown>
        ) : null}
      </div>
    );
  }

  const text = normalizeCommentMarkdown(trimmed);
  if (!text) return null;

  const comps =
    basePath || ticketId
      ? markdownComponents({ mermaidAsCode, basePath, ticketId })
      : mermaidAsCode
        ? mermaidAsCodeComponents
        : components;

  return (
    <ReactMarkdown
      remarkPlugins={[remarkGfm, remarkFileReferences]}
      components={comps}
      urlTransform={urlTransform}
    >
      {text}
    </ReactMarkdown>
  );
}
