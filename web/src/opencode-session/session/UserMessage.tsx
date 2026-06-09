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
    <div className={`rounded-md border border-border bg-surface px-3 py-2 ${sessionTheme.text}`}>
      <p className="whitespace-pre-wrap font-body text-sm">{text}</p>
      {message.error?.data?.message && (
        <p className={`mt-1 font-body text-xs ${sessionTheme.error}`}>
          {message.error.data.message}
        </p>
      )}
    </div>
  );
}
