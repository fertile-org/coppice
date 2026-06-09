import ReactMarkdown from 'react-markdown';
import type { TextPart as TextPartType } from '../sync/types';
import { sessionTheme } from '../theme/session-theme';

export function TextPart({ part }: { part: TextPartType }) {
  const text = part.text.trim();
  if (!text) return null;
  return (
    <div className={`ml-3 mt-2 ${sessionTheme.text}`}>
      <ReactMarkdown>{text}</ReactMarkdown>
    </div>
  );
}
