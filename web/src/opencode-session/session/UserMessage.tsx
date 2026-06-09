import { SessionSectionCard } from '../components/SessionSectionCard';
import type { Message, Part, TextPart as TextPartType } from '../sync/types';
import { sessionTheme } from '../theme/session-theme';

function isTextPart(part: Part): part is TextPartType {
  return part.type === 'text';
}

export function UserMessage({
  message,
  parts,
}: {
  message: Message;
  parts: Part[];
}) {
  const text = parts
    .filter(isTextPart)
    .map((part) => part.text)
    .join('')
    .trim();

  if (!text) return null;

  return (
    <SessionSectionCard>
      <div className={sessionTheme.fontMono}>
        <p className={`whitespace-pre-wrap ${sessionTheme.text}`}>
          <span className={sessionTheme.thinking}>{'> '}</span>
          {text}
        </p>
        {message.error?.data?.message && (
          <p className={`mt-1 ${sessionTheme.error}`}>
            {message.error.data.message}
          </p>
        )}
      </div>
    </SessionSectionCard>
  );
}
