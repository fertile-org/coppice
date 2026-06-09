import type { Components } from 'react-markdown';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { sessionTheme } from '../theme/session-theme';
import { CodeBlock } from './CodeBlock';
import { OutputBlock } from './OutputBlock';

function markdownComponents(textClass = ''): Components {
  return {
  p: ({ children }) => (
    <p className={`mb-2 last:mb-0 ${sessionTheme.fontMono} ${textClass}`}>{children}</p>
  ),
  ul: ({ children }) => (
    <ul className={`mb-2 list-disc space-y-1 pl-5 last:mb-0 ${sessionTheme.fontMono} ${textClass}`}>
      {children}
    </ul>
  ),
  ol: ({ children }) => (
    <ol className={`mb-2 list-decimal space-y-1 pl-5 last:mb-0 ${sessionTheme.fontMono} ${textClass}`}>
      {children}
    </ol>
  ),
  li: ({ children }) => <li className={`leading-[var(--oc-line-height)] ${textClass}`}>{children}</li>,
  h1: ({ children }) => (
    <h1
      className={`mb-2 mt-3 font-semibold first:mt-0 ${sessionTheme.fontMono} ${sessionTheme.markdownHeading}`}
    >
      {children}
    </h1>
  ),
  h2: ({ children }) => (
    <h2
      className={`mb-2 mt-3 font-semibold first:mt-0 ${sessionTheme.fontMono} ${sessionTheme.markdownHeading}`}
    >
      {children}
    </h2>
  ),
  h3: ({ children }) => (
    <h3
      className={`mb-1 mt-2 font-semibold first:mt-0 ${sessionTheme.fontMono} ${sessionTheme.markdownHeading}`}
    >
      {children}
    </h3>
  ),
  strong: ({ children }) => (
    <strong className={`font-semibold ${sessionTheme.markdownStrong}`}>{children}</strong>
  ),
  em: ({ children }) => <em className={`italic ${sessionTheme.markdownEmph}`}>{children}</em>,
  a: ({ href, children }) => (
    <a
      href={href}
      className={`${sessionTheme.markdownLink} underline-offset-2 hover:underline`}
      target="_blank"
      rel="noreferrer"
    >
      {children}
    </a>
  ),
  blockquote: ({ children }) => (
    <blockquote className={`mb-2 pl-3 last:mb-0 ${sessionTheme.markdownBlockquote}`}>
      {children}
    </blockquote>
  ),
  code: ({ className, children }) => {
    const text = String(children).replace(/\n$/, '');
    const match = /language-(\w+)/.exec(className ?? '');
    if (match) {
      return (
        <OutputBlock className="my-2">
          <CodeBlock code={text} language={match[1]} />
        </OutputBlock>
      );
    }
    return (
      <code
        className={`px-0.5 font-mono text-[length:var(--oc-font-size-sm)] ${sessionTheme.markdownCode}`}
      >
        {text}
      </code>
    );
  },
  pre: ({ children }) => <>{children}</>,
  table: ({ children }) => (
    <div className="my-2 overflow-x-auto">
      <table className={`min-w-full border-collapse ${sessionTheme.fontMono}`}>{children}</table>
    </div>
  ),
  th: ({ children }) => (
    <th
      className={`border border-[var(--oc-border)] ${sessionTheme.bgElement} px-2 py-1 text-left font-semibold`}
    >
      {children}
    </th>
  ),
  td: ({ children }) => (
    <td className={`border border-[var(--oc-border)] px-2 py-1 ${textClass || sessionTheme.text}`}>
      {children}
    </td>
  ),
};
}

export function MarkdownContent({
  children,
  className = '',
  tone = 'default',
}: {
  children: string;
  className?: string;
  tone?: 'default' | 'thinking';
}) {
  const text = children.trim();
  if (!text) return null;

  const textClass = tone === 'thinking' ? sessionTheme.thinkingDetail : '';

  return (
    <div className={className}>
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents(textClass)}>
        {text}
      </ReactMarkdown>
    </div>
  );
}
