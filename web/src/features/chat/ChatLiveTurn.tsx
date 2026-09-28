import '../../opencode-session/theme/opencode-theme.css';
import { MarkdownContent } from '../../opencode-session/components/MarkdownContent';
import { AssistantMessage } from '../../opencode-session/session/AssistantMessage';
import { UserMessage } from '../../opencode-session/session/UserMessage';
import { sessionTheme } from '../../opencode-session/theme/session-theme';
import type { SessionStore } from '../../opencode-session/sync/types';
import type { ClaudeConsoleEntry } from '../runs/claude-console-state';
import { ThinkingIndicator } from './ChatMessageList';
import { useChatRunLiveStream } from './useChatRunLiveStream';

function ChatStreamView({ store }: { store: SessionStore }) {
  const messages = [...store.messages].sort((a, b) => a.id.localeCompare(b.id));
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
    </div>
  );
}

function ChatConsolePreview({ entries }: { entries: ClaudeConsoleEntry[] }) {
  const visible = entries.filter((entry) => entry.kind !== 'session');
  if (visible.length === 0) return null;

  const textBlocks = visible
    .filter((entry): entry is Extract<ClaudeConsoleEntry, { kind: 'text' }> =>
      entry.kind === 'text',
    )
    .map((entry) => entry.markdown)
    .join('\n\n')
    .trim();

  const tools = visible.filter(
    (entry): entry is Extract<ClaudeConsoleEntry, { kind: 'tool' }> =>
      entry.kind === 'tool',
  );

  const result = visible.find(
    (entry): entry is Extract<ClaudeConsoleEntry, { kind: 'result' }> =>
      entry.kind === 'result',
  );

  const continued = visible.find(
    (entry): entry is Extract<ClaudeConsoleEntry, { kind: 'continued' }> =>
      entry.kind === 'continued',
  );

  const body =
    textBlocks ||
    result?.contract.summary ||
    continued?.summary ||
    '';

  return (
    <div className="flex justify-start">
      <article
        aria-label="Agent"
        className="max-w-[85%] rounded-2xl rounded-bl-md border border-moss-200 bg-moss-50 px-3 py-1.5 font-body text-sm text-text-primary"
        data-testid="chat-console-preview"
      >
        {tools.length > 0 ? (
          <ul className="mb-1.5 space-y-0.5 font-body text-xs text-text-secondary">
            {tools.map((tool) => (
              <li key={tool.id}>
                {tool.status === 'running' ? 'Using' : 'Used'}{' '}
                {tool.title || 'tool'}
                {tool.status === 'running' ? '…' : ''}
              </li>
            ))}
          </ul>
        ) : null}
        {body ? (
          <div className="leading-snug">
            <MarkdownContent>{body}</MarkdownContent>
          </div>
        ) : null}
      </article>
    </div>
  );
}

export function ChatLiveTurn({
  runId,
  runStatus = 'running',
  onFinished,
}: {
  runId: string;
  runStatus?: string | null;
  onFinished?: () => void;
}) {
  const live = useChatRunLiveStream(runId, { runStatus, onFinished });

  return (
    <div className="flex flex-col gap-2" data-testid="chat-live-turn">
      {live.awaiting ? (
        <ThinkingIndicator label={live.thinkingLabel} />
      ) : null}
      {live.hasConsole ? (
        <ChatConsolePreview entries={live.consoleEntries} />
      ) : null}
      {live.hasOpenCode && live.store ? (
        <div
          className={`oc-session rounded-lg border border-[var(--oc-border)] px-3 py-3 ${sessionTheme.bg}`}
          data-testid="chat-opencode-stream"
        >
          <ChatStreamView store={live.store} />
        </div>
      ) : null}
    </div>
  );
}
