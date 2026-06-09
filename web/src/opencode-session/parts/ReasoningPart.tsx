import ReactMarkdown from 'react-markdown';
import type { ReasoningPart as ReasoningPartType } from '../sync/types';
import { sessionTheme } from '../theme/session-theme';

export function ReasoningPart({ part }: { part: ReasoningPartType }) {
  const content = part.text.replaceAll('[REDACTED]', '').trim();
  if (!content) return null;
  return (
    <div
      className={`ml-2 mt-2 border-l-2 pl-2 ${sessionTheme.border} ${sessionTheme.textMuted}`}
    >
      <ReactMarkdown>{`_Thinking:_ ${content}`}</ReactMarkdown>
    </div>
  );
}
