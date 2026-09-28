import { useEffect, useMemo, useReducer, useRef, useState } from 'react';
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

export type ChatRunLiveConnection =
  | 'connecting'
  | 'open'
  | 'closed'
  | 'reconnecting';

export interface ChatRunLiveState {
  text: string;
  connection: ChatRunLiveConnection;
  error: string | null;
  finished: boolean;
  store: SessionStore | null;
  consoleEntries: ClaudeConsoleEntry[];
  awaiting: boolean;
  thinkingLabel: string;
  hasOpenCode: boolean;
  hasConsole: boolean;
}

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

function consoleHasVisibleBody(entries: ClaudeConsoleEntry[]): boolean {
  return entries.some((entry) => entry.kind !== 'session');
}

export function consoleStreamText(entries: ClaudeConsoleEntry[]): string {
  const visible = entries.filter((entry) => entry.kind !== 'session');
  if (visible.length === 0) return '';

  const textBlocks = visible
    .filter((entry): entry is Extract<ClaudeConsoleEntry, { kind: 'text' }> =>
      entry.kind === 'text',
    )
    .map((entry) => entry.markdown)
    .join('\n\n')
    .trim();

  const result = visible.find(
    (entry): entry is Extract<ClaudeConsoleEntry, { kind: 'result' }> =>
      entry.kind === 'result',
  );

  const continued = visible.find(
    (entry): entry is Extract<ClaudeConsoleEntry, { kind: 'continued' }> =>
      entry.kind === 'continued',
  );

  return (
    textBlocks ||
    result?.contract.summary ||
    continued?.summary ||
    ''
  );
}

export function openCodeStreamText(store: SessionStore | null): string {
  if (!store) return '';
  const messages = [...store.messages].sort((a, b) => a.id.localeCompare(b.id));
  const chunks = messages
    .filter((message) => message.role === 'assistant')
    .map((message) => {
      const parts = store.parts[message.id] ?? [];
      return parts
        .filter((part): part is Extract<Part, { type: 'text' }> => part.type === 'text')
        .map((part) => part.text)
        .join('');
    })
    .filter((text) => text.trim().length > 0);
  return chunks.join('\n\n').trim();
}

export function mergeLiveStreamText(
  store: SessionStore | null,
  consoleEntries: ClaudeConsoleEntry[],
): string {
  const consoleText = consoleStreamText(consoleEntries);
  if (consoleText) return consoleText;
  return openCodeStreamText(store);
}

export function useChatRunLiveStream(
  runId: string | null,
  options?: { runStatus?: string | null; onFinished?: () => void },
): ChatRunLiveState {
  const runStatus = options?.runStatus ?? 'running';
  const onFinished = options?.onFinished;

  const [store, dispatchSession] = useReducer(sessionReducer, null);
  const [consoleState, dispatchConsole] = useReducer(
    consoleReducer,
    null,
    createClaudeConsoleState,
  );
  const [connection, setConnection] = useState<ChatRunLiveConnection>('closed');
  const [reconnectToken, setReconnectToken] = useState(0);
  const [sessionStatus, setSessionStatus] = useState<string | null>(null);
  const [finished, setFinished] = useState(false);
  const hasStreamRef = useRef(false);
  const recoverableRef = useRef(true);
  const finishedRef = useRef(false);

  useEffect(() => {
    if (!runId) {
      setConnection('closed');
      setFinished(false);
      return;
    }
    recoverableRef.current = true;
    finishedRef.current = false;
    hasStreamRef.current = false;
    setSessionStatus(null);
    setFinished(false);
    dispatchSession({ type: 'reset', sessionId: runId });
    dispatchConsole({ type: 'reset' });
  }, [runId]);

  useEffect(() => {
    if (!runId) return;

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
          if (typeof ty === 'string' && !ty.endsWith('.console.session')) {
            hasStreamRef.current = true;
          }
        } else if (isOpenCodeEvent(liveEvent)) {
          const nextStatus = sessionStatusFromEvent(
            liveEvent as unknown as OpenCodeEvent,
          );
          if (nextStatus) setSessionStatus(nextStatus);
          dispatchSession({
            type: 'event',
            event: liveEvent as unknown as OpenCodeEvent,
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
          setFinished(true);
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
    if (!runId) return;
    if (!isActiveRunStatus(runStatus) || connection !== 'closed') return;
    if (!recoverableRef.current) return;
    const timer = window.setTimeout(() => {
      setReconnectToken((token) => token + 1);
    }, 800);
    return () => window.clearTimeout(timer);
  }, [runId, runStatus, connection]);

  const hasOpenCode = openCodeHasMessages(store);
  const hasConsole = consoleHasVisibleBody(consoleState.entries);
  const hasRenderable = hasOpenCode || hasConsole;

  const awaiting =
    Boolean(runId) &&
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

  const text = useMemo(
    () => mergeLiveStreamText(store, consoleState.entries),
    [store, consoleState.entries],
  );

  return {
    text,
    connection,
    error: null,
    finished,
    store,
    consoleEntries: consoleState.entries,
    awaiting,
    thinkingLabel,
    hasOpenCode,
    hasConsole,
  };
}
