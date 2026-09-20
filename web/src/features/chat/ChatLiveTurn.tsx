import { useEffect, useReducer, useRef, useState } from 'react';
import '../../opencode-session/theme/opencode-theme.css';
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
import { ThinkingIndicator } from './ChatMessageList';

type ConnectionState = 'connecting' | 'open' | 'closed' | 'reconnecting';

type SessionAction =
  | { type: 'reset'; sessionId: string }
  | { type: 'snapshot'; snapshot: SessionSnapshot }
  | { type: 'event'; event: OpenCodeEvent };

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

export function ChatLiveTurn({
  runId,
  runStatus = 'running',
  onFinished,
}: {
  runId: string;
  runStatus?: string | null;
  onFinished?: () => void;
}) {
  const [store, dispatch] = useReducer(sessionReducer, null);
  const [connection, setConnection] = useState<ConnectionState>('closed');
  const [reconnectToken, setReconnectToken] = useState(0);
  const [hasContent, setHasContent] = useState(false);
  const [sessionStatus, setSessionStatus] = useState<string | null>(null);
  const hasContentRef = useRef(false);
  const recoverableRef = useRef(true);
  const finishedRef = useRef(false);

  useEffect(() => {
    recoverableRef.current = true;
    finishedRef.current = false;
    setHasContent(false);
    hasContentRef.current = false;
    setSessionStatus(null);
    dispatch({ type: 'reset', sessionId: runId });
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
        event?: OpenCodeEvent;
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
        dispatch({
          type: 'snapshot',
          snapshot: {
            sessionId: msg.sessionId ?? runId,
            messages: msg.messages ?? [],
            parts: msg.parts ?? {},
          },
        });
        hasContentRef.current = true;
        setHasContent(true);
      } else if (msg.type === 'event' && msg.event) {
        const nextStatus = sessionStatusFromEvent(msg.event);
        if (nextStatus) setSessionStatus(nextStatus);
        dispatch({ type: 'event', event: msg.event });
        hasContentRef.current = true;
        setHasContent(true);
      } else if (msg.type === 'end') {
        if (shouldStopReconnect(msg)) {
          recoverableRef.current = false;
        }
        if (
          !hasContentRef.current &&
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

  const awaiting =
    !hasContent &&
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
    <div
      className={`oc-session rounded-lg border border-[var(--oc-border)] px-3 py-3 ${sessionTheme.bg}`}
      data-testid="chat-live-turn"
    >
      {awaiting && <ThinkingIndicator label={thinkingLabel} />}
      {store && hasContent ? <ChatStreamView store={store} /> : null}
    </div>
  );
}
