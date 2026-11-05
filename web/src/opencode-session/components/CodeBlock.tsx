import { useMemo } from 'react';
import { hljs } from '../../lib/highlight';
import { sessionTheme } from '../theme/session-theme';

export function CodeBlock({
  code,
  language,
  className = '',
}: {
  code: string;
  language?: string;
  className?: string;
}) {
  const html = useMemo(() => {
    if (!code.trim()) return '';
    const lang = language && hljs.getLanguage(language) ? language : undefined;
    if (lang) {
      return hljs.highlight(code, { language: lang }).value;
    }
    return hljs.highlightAuto(code).value;
  }, [code, language]);

  return (
    <pre className={`overflow-x-auto ${className}`}>
      <code
        className={`hljs font-mono text-[length:var(--oc-font-size-sm)] leading-[var(--oc-line-height)] ${sessionTheme.text}`}
        dangerouslySetInnerHTML={{ __html: html }}
      />
    </pre>
  );
}
