import { useEffect, useReducer, useRef, useState } from 'react';
import '../../opencode-session/theme/opencode-theme.css';
import { MarkdownContent } from '../../opencode-session/components/MarkdownContent';
import { AssistantMessage } from '../../opencode-session/session/AssistantMessage';
import { UserMessage } from '../../opencode-session/session/UserMessage';
import { sessionTheme } from '../../opencode-session/theme/session-theme';
import {
  applyEvent,
  applySnapshot,
  cloneSessionStore,
  createSessionStore,
} from '../../opencode-session/sync/reduce-event';
import type {
  Message,
  OpenCodeEvent,
  Part,
  SessionSnapshot,
  SessionStore,
} from '../../opencode-session/sync/types';
import {
  applyClaudeConsoleEvent,
  createClaudeConsoleState,
  resetClaudeConsoleState,
  type ClaudeConsoleEntry,
  type ClaudeConsoleState,
} from '../runs/claude-console-state';
import { ThinkingIndicator } from './ChatMessageList';

type ConnectionState = 'connecting' | 'open' | 'closed' | 'reconnecting';

type SessionAction =
  | { type: 'reset'; sessionId: string }
  | { type: 'snapshot'; snapshot: SessionSnapshot }
  | { type: 'event'; event: OpenCodeEvent };

type ConsoleAction =
  | { type: 'reset' }
  | { type: 'event'; event: Record<string, unknown> };

function sessionReducer(
  state: SessionStore | null,
  action: SessionAction,
): SessionStore | null {
  switch (action.type) {
    case 'reset':
      return createSessionStore(action.sessionId);
    case 'snapshot': {
      const base = state ?? createSessionStore(action.snapshot.sessionId);
      const next = cloneSessionStore(base);
      applySnapshot(next, action.snapshot);
      return next;
    }
    case 'event': {
      if (!state) return state;
      const next = cloneSessionStore(state);
      applyEvent(next, action.event);
      return next;
    }
    default:
      return state;
  }
}

function consoleReducer(
  state: ClaudeConsoleState,
  action: ConsoleAction,
): ClaudeConsoleState {
  switch (action.type) {
    case 'reset':
      return resetClaudeConsoleState();
    case 'event':
      return applyClaudeConsoleEvent(state, action.event);
    default:
      return state;
  }
}

function isActiveRunStatus(status: string | null): boolean {
  return status === 'running' || status === 'queued';
}

function shouldStopReconnect(msg: {
  recoverable?: boolean;
  reason?: string | null;
}): boolean {
  if (msg.recoverable === false) return true;
  return (
    typeof msg.reason === 'string' && msg.reason.includes('interrupted')
  );
}

function sessionStatusFromEvent(event: OpenCodeEvent): string | null {
  if (event.type !== 'session.status') return null;
  const props = event.properties;
  if (!props || typeof props !== 'object') return null;
  const status = (props as { status?: { type?: string } }).status;
  return typeof status?.type === 'string' ? status.type : null;
}

function isConsoleEvent(event: Record<string, unknown>): boolean {
  const ty = event.type;
  return typeof ty === 'string' && ty.includes('.console.');
}

function isOpenCodeEvent(event: Record<string, unknown>): boolean {
  const ty = event.type;
  return (
    typeof ty === 'string' &&
    (ty === 'message.updated' ||
      ty === 'message.part.updated' ||
      ty === 'message.part.delta' ||
      ty === 'session.status')
  );
}

function openCodeHasMessages(store: SessionStore | null): boolean {
  return Boolean(store && store.messages.length > 0);
}

/** Session-start alone is not visible body — keep the thinking indicator. */
function consoleHasVisibleBody(entries: ClaudeConsoleEntry[]): boolean {
  return entries.some((entry) => entry.kind !== 'session');
}

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

/** Light Coppice chrome for Cursor/Claude/Kilo `*.console.*` live turns. */
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
    <article
      className="mr-8 rounded-lg border border-border bg-paper-100 px-3 py-2"
      data-testid="chat-console-preview"
    >
      <header className="mb-1 font-body text-xs font-medium text-text-secondary">
        Agent
      </header>
      {tools.length > 0 ? (
        <ul className="mb-2 space-y-0.5 font-body text-xs text-text-secondary">
          {tools.map((tool) => (
            <li key={tool.id}>
              {tool.status === 'running' ? 'Using' : 'Used'} {tool.title || 'tool'}
              {tool.status === 'running' ? '…' : ''}
            </li>
          ))}
        </ul>
      ) : null}
      {body ? (
        <div className="font-body text-sm text-text-primary">
          <MarkdownContent>{body}</MarkdownContent>
        </div>
      ) : null}
    </article>
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
  const [store, dispatchSession] = useReducer(sessionReducer, null);
  const [consoleState, dispatchConsole] = useReducer(
    consoleReducer,
    null,
    createClaudeConsoleState,
  );
  const [connection, setConnection] = useState<ConnectionState>('closed');
  const [reconnectToken, setReconnectToken] = useState(0);
  const [sessionStatus, setSessionStatus] = useState<string | null>(null);
  const hasStreamRef = useRef(false);
  const recoverableRef = useRef(true);
  const finishedRef = useRef(false);

  useEffect(() => {
    recoverableRef.current = true;
    finishedRef.current = false;
    hasStreamRef.current = false;
    setSessionStatus(null);
    dispatchSession({ type: 'reset', sessionId: runId });
    dispatchConsole({ type: 'reset' });
  }, [runId]);

  useEffect(() => {
    setConnection('connecting');

    let active = true;
    const protocol = window.location.protocol === 'https:' ? 'wss' : 'ws';
    const ws = new WebSocket(
      `${protocol}://${window.location.host}/ws/agent-runs/${runId}/live`,
    );

    ws.onopen = () => {
      if (active) setConnection('open');
    };
    ws.onclose = () => {
      if (active) setConnection('closed');
    };
    ws.onerror = () => {
      if (active) setConnection('closed');
    };
    ws.onmessage = (event) => {
      const msg = JSON.parse(event.data as string) as {
        type?: string;
        messages?: Message[];
        parts?: Record<string, Part[]>;
        sessionId?: string;
        event?: Record<string, unknown>;
        recoverable?: boolean;
        reason?: string | null;
        status?: string;
        sessionStatus?: string;
      };

      if (msg.type === 'heartbeat') {
        if (typeof msg.sessionStatus === 'string') {
          setSessionStatus(msg.sessionStatus);
        }
        return;
      }

      if (msg.type === 'snapshot') {
        dispatchSession({
          type: 'snapshot',
          snapshot: {
            sessionId: msg.sessionId ?? runId,
            messages: msg.messages ?? [],
            parts: msg.parts ?? {},
          },
        });
        if ((msg.messages ?? []).length > 0) {
          hasStreamRef.current = true;
        }
      } else if (msg.type === 'event' && msg.event) {
        const liveEvent = msg.event;
        if (isConsoleEvent(liveEvent)) {
          dispatchConsole({ type: 'event', event: liveEvent });
          const ty = liveEvent.type;
          // Session-start alone is not enough to count as stream body.
          if (
            typeof ty === 'string' &&
            !ty.endsWith('.console.session')
          ) {
            hasStreamRef.current = true;
          }
        } else if (isOpenCodeEvent(liveEvent)) {
          const nextStatus = sessionStatusFromEvent(
            liveEvent as OpenCodeEvent,
          );
          if (nextStatus) setSessionStatus(nextStatus);
          dispatchSession({
            type: 'event',
            event: liveEvent as OpenCodeEvent,
          });
          if (
            liveEvent.type === 'message.updated' ||
            liveEvent.type === 'message.part.updated' ||
            liveEvent.type === 'message.part.delta'
          ) {
            hasStreamRef.current = true;
          }
        }
      } else if (msg.type === 'end') {
        if (shouldStopReconnect(msg)) {
          recoverableRef.current = false;
        }
        if (
          !hasStreamRef.current &&
          msg.status &&
          isActiveRunStatus(msg.status) &&
          recoverableRef.current
        ) {
          ws.close();
          return;
        }
        if (!finishedRef.current) {
          finishedRef.current = true;
          onFinished?.();
        }
        ws.close();
      }
    };

    return () => {
      active = false;
      ws.close();
    };
  }, [runId, reconnectToken, onFinished]);

  useEffect(() => {
    if (!isActiveRunStatus(runStatus) || connection !== 'closed') return;
    if (!recoverableRef.current) return;
    const timer = window.setTimeout(() => {
      setReconnectToken((token) => token + 1);
    }, 800);
    return () => window.clearTimeout(timer);
  }, [runStatus, connection]);

  const hasOpenCode = openCodeHasMessages(store);
  const hasConsole = consoleHasVisibleBody(consoleState.entries);
  const hasRenderable = hasOpenCode || hasConsole;

  const awaiting =
    !hasRenderable &&
    (connection === 'connecting' ||
      connection === 'open' ||
      connection === 'reconnecting' ||
      isActiveRunStatus(runStatus));

  const thinkingLabel =
    sessionStatus === 'busy' || sessionStatus === 'retry'
      ? 'Thinking…'
      : awaiting
        ? 'Waiting for reply…'
        : 'Thinking…';

  return (
    <div className="flex flex-col gap-2" data-testid="chat-live-turn">
      {awaiting ? <ThinkingIndicator label={thinkingLabel} /> : null}
      {hasConsole ? (
        <ChatConsolePreview entries={consoleState.entries} />
      ) : null}
      {hasOpenCode && store ? (
        <div
          className={`oc-session rounded-lg border border-[var(--oc-border)] px-3 py-3 ${sessionTheme.bg}`}
          data-testid="chat-opencode-stream"
        >
          <ChatStreamView store={store} />
        </div>
      ) : null}
    </div>
  );
}
