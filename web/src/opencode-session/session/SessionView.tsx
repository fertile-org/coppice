import { useMemo, useState } from 'react';
import type { SessionStore } from '../sync/types';
import { sessionTheme } from '../theme/session-theme';
import { AssistantMessage } from './AssistantMessage';
import { SessionFooter } from './SessionFooter';
import { UserMessage } from './UserMessage';

/** Session messages mounted at once. */
export const SESSION_RENDER_WINDOW = 300;

export function SessionView({ store }: { store: SessionStore }) {
  const messages = useMemo(
    () => [...store.messages].sort((a, b) => a.id.localeCompare(b.id)),
    [store.messages],
  );
  // Only the newest messages are mounted; older ones are one click away.
  const [shown, setShown] = useState(SESSION_RENDER_WINDOW);
  const hidden = Math.max(0, messages.length - shown);
  const visible = hidden > 0 ? messages.slice(hidden) : messages;

  return (
    <div className={`flex flex-col ${sessionTheme.sectionGap}`}>
      {hidden > 0 && (
        <button
          type="button"
          onClick={() => setShown((count) => count + SESSION_RENDER_WINDOW)}
          className={`self-start ${sessionTheme.fontBody} text-xs ${sessionTheme.textMuted} underline`}
        >
          Show earlier output ({hidden})
        </button>
      )}
      {visible.map((message) =>
        message.role === 'assistant' ? (
          <AssistantMessage
            key={message.id}
            message={message}
            parts={store.parts[message.id] ?? []}
          />
        ) : (
          <UserMessage
            key={message.id}
            message={message}
            parts={store.parts[message.id] ?? []}
          />
        ),
      )}
      <SessionFooter messages={messages} />
    </div>
  );
}
