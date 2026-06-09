import { CodeBlock } from './CodeBlock';
import { MarkdownContent } from './MarkdownContent';
import { languageFromPath } from '../tools/parse-opencode-output';

export function FileContentView({
  path,
  content,
}: {
  path?: string;
  content: string;
}) {
  const language = path ? languageFromPath(path) : undefined;

  if (language === 'markdown') {
    return <MarkdownContent>{content}</MarkdownContent>;
  }

  return <CodeBlock code={content} language={language} />;
}
