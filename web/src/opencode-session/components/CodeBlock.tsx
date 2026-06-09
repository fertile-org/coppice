import 'highlight.js/styles/github.min.css';
import hljs from 'highlight.js/lib/core';
import bash from 'highlight.js/lib/languages/bash';
import css from 'highlight.js/lib/languages/css';
import go from 'highlight.js/lib/languages/go';
import ini from 'highlight.js/lib/languages/ini';
import javascript from 'highlight.js/lib/languages/javascript';
import json from 'highlight.js/lib/languages/json';
import markdown from 'highlight.js/lib/languages/markdown';
import python from 'highlight.js/lib/languages/python';
import rust from 'highlight.js/lib/languages/rust';
import sql from 'highlight.js/lib/languages/sql';
import typescript from 'highlight.js/lib/languages/typescript';
import xml from 'highlight.js/lib/languages/xml';
import yaml from 'highlight.js/lib/languages/yaml';
import { useMemo } from 'react';

hljs.registerLanguage('bash', bash);
hljs.registerLanguage('css', css);
hljs.registerLanguage('go', go);
hljs.registerLanguage('ini', ini);
hljs.registerLanguage('javascript', javascript);
hljs.registerLanguage('json', json);
hljs.registerLanguage('markdown', markdown);
hljs.registerLanguage('python', python);
hljs.registerLanguage('rust', rust);
hljs.registerLanguage('sql', sql);
hljs.registerLanguage('typescript', typescript);
hljs.registerLanguage('xml', xml);
hljs.registerLanguage('yaml', yaml);

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
    <pre
      className={`hljs overflow-x-auto rounded-md border border-border bg-surface-raised px-3 py-2 text-xs leading-relaxed ${className}`}
    >
      <code className="hljs font-mono" dangerouslySetInnerHTML={{ __html: html }} />
    </pre>
  );
}
