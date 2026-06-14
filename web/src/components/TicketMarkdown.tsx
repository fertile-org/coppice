import type { Components } from 'react-markdown';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

const components: Components = {
  h1: ({ children }) => (
    <h1 className="mb-3 mt-5 font-display text-lg font-semibold text-text-primary first:mt-0">
      {children}
    </h1>
  ),
  h2: ({ children }) => (
    <h2 className="mb-2 mt-5 font-display text-base font-semibold text-text-primary first:mt-0">
      {children}
    </h2>
  ),
  h3: ({ children }) => (
    <h3 className="mb-2 mt-4 font-body text-sm font-semibold text-text-primary first:mt-0">
      {children}
    </h3>
  ),
  p: ({ children }) => (
    <p className="mb-3 font-body text-sm leading-relaxed text-text-primary last:mb-0">
      {children}
    </p>
  ),
  ul: ({ children }) => (
    <ul className="mb-3 list-disc space-y-1.5 pl-5 font-body text-sm leading-relaxed text-text-primary">
      {children}
    </ul>
  ),
  ol: ({ children }) => (
    <ol className="mb-3 list-decimal space-y-1.5 pl-5 font-body text-sm leading-relaxed text-text-primary">
      {children}
    </ol>
  ),
  li: ({ children }) => <li className="leading-relaxed">{children}</li>,
  strong: ({ children }) => (
    <strong className="font-semibold text-text-primary">{children}</strong>
  ),
  a: ({ href, children }) => (
    <a href={href} className="text-accent underline-offset-2 hover:underline">
      {children}
    </a>
  ),
  code: ({ children }) => (
    <code className="rounded bg-paper-200 px-1 font-mono text-[0.9em]">
      {children}
    </code>
  ),
  pre: ({ children }) => (
    <pre className="mb-3 overflow-x-auto rounded-md border border-border bg-paper-200 p-3 font-mono text-xs leading-relaxed">
      {children}
    </pre>
  ),
  blockquote: ({ children }) => (
    <blockquote className="mb-3 border-l-2 border-border pl-3 text-text-secondary">
      {children}
    </blockquote>
  ),
  table: ({ children }) => (
    <div className="mb-4 overflow-x-auto">
      <table className="min-w-full border-collapse font-body text-sm">{children}</table>
    </div>
  ),
  thead: ({ children }) => <thead className="bg-paper-200">{children}</thead>,
  th: ({ children }) => (
    <th className="border border-border px-3 py-2 text-left font-semibold text-text-primary">
      {children}
    </th>
  ),
  td: ({ children }) => (
    <td className="border border-border px-3 py-2 align-top text-text-primary">
      {children}
    </td>
  ),
};

export function normalizeCommentMarkdown(text: string): string {
  // Ensure markdown block sections (e.g. **Tests run:**) start on their own paragraph.
  return text.replace(/([^\n])\n(\*\*[^*]+:\*\*)/g, '$1\n\n$2');
}

export function TicketMarkdown({ children }: { children: string }) {
  const text = normalizeCommentMarkdown(children.trim());
  if (!text) return null;

  return (
    <ReactMarkdown remarkPlugins={[remarkGfm]} components={components}>
      {text}
    </ReactMarkdown>
  );
}
