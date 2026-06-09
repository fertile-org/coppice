import { sessionTheme } from '../theme/session-theme';
import { CodeBlock } from './CodeBlock';

export function PlainOutput({ text, language }: { text: string; language?: string }) {
  const trimmed = text.trim();
  if (!trimmed) return null;

  if (language) {
    return <CodeBlock code={trimmed} language={language} />;
  }

  return (
    <pre className={`overflow-x-auto whitespace-pre-wrap ${sessionTheme.text}`}>
      {trimmed}
    </pre>
  );
}
