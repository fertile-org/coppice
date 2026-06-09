import { useMemo } from 'react';
import type { SessionStore } from '../sync/types';
import { sessionTheme } from '../theme/session-theme';
import { AssistantMessage } from './AssistantMessage';
import { SessionFooter } from './SessionFooter';
import { UserMessage } from './UserMessage';

export function SessionView({ store }: { store: SessionStore }) {
  const messages = useMemo(
    () => [...store.messages].sort((a, b) => a.id.localeCompare(b.id)),
    [store.messages],
  );

  return (
    <div className={`flex flex-col ${sessionTheme.sectionGap}`}>
      {messages.map((message) =>
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
