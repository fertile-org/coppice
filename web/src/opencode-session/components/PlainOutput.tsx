import { CodeBlock } from './CodeBlock';

export function PlainOutput({ text, language }: { text: string; language?: string }) {
  const trimmed = text.trim();
  if (!trimmed) return null;

  if (language) {
    return <CodeBlock code={trimmed} language={language} />;
  }

  return (
    <pre className="overflow-x-auto whitespace-pre-wrap rounded-md border border-border bg-surface-raised px-3 py-2 font-mono text-xs leading-relaxed text-text-primary">
      {trimmed}
    </pre>
  );
}
