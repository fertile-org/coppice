import { MarkdownContent } from '../components/MarkdownContent';
import type { TextPart as TextPartType } from '../sync/types';
import { sessionTheme } from '../theme/session-theme';

export function TextPart({ part }: { part: TextPartType }) {
  const content = part.text.trim();
  if (!content) return null;

  return (
    <div className={`ml-3 mt-2 ${sessionTheme.text}`}>
      <MarkdownContent>{content}</MarkdownContent>
    </div>
  );
}
