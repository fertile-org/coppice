import { useMemo } from 'react';
import type { SessionStore } from '../sync/types';
import { AssistantMessage } from './AssistantMessage';
import { UserMessage } from './UserMessage';

export function SessionView({ store }: { store: SessionStore }) {
  const messages = useMemo(
    () => [...store.messages].sort((a, b) => a.id.localeCompare(b.id)),
    [store.messages],
  );

  return (
    <div className="flex flex-col gap-3 overflow-y-auto">
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
    </div>
  );
}
