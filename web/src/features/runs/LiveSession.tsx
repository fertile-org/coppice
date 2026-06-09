import { useEffect, useReducer, useRef, useState } from 'react';
import { SessionView } from '../../opencode-session/session/SessionView';
import {
  applyEvent,
  applySnapshot,
  createSessionStore,
} from '../../opencode-session/sync/reduce-event';
import type {
  Message,
  OpenCodeEvent,
  Part,
  SessionSnapshot,
  SessionStore,
} from '../../opencode-session/sync/types';

interface LiveSessionProps {
  runId: string | null;
  runStatus: string | null;
}

type ConnectionState = 'connecting' | 'open' | 'closed';

type SessionAction =
  | { type: 'reset'; sessionId: string }
  | { type: 'snapshot'; snapshot: SessionSnapshot }
  | { type: 'event'; event: OpenCodeEvent };

function cloneStore(store: SessionStore): SessionStore {
  return {
    sessionId: store.sessionId,
    messages: [...store.messages],
    parts: Object.fromEntries(
      Object.entries(store.parts).map(([messageId, parts]) => [
        messageId,
        [...parts],
      ]),
    ),
    pendingDeltas: Object.fromEntries(
      Object.entries(store.pendingDeltas).map(([partId, deltas]) => [
        partId,
        [...deltas],
      ]),
    ),
  };
}

function sessionReducer(
  state: SessionStore | null,
  action: SessionAction,
): SessionStore | null {
  switch (action.type) {
    case 'reset':
      return createSessionStore(action.sessionId);
    case 'snapshot': {
      const base = state ?? createSessionStore(action.snapshot.sessionId);
      const next = cloneStore(base);
      applySnapshot(next, action.snapshot);
      return next;
    }
    case 'event': {
      if (!state) return state;
      const next = cloneStore(state);
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

export function LiveSession({ runId, runStatus }: LiveSessionProps) {
  const [store, dispatch] = useReducer(sessionReducer, null);
  const [connection, setConnection] = useState<ConnectionState>('closed');
  const [reconnectToken, setReconnectToken] = useState(0);
  const [interrupted, setInterrupted] = useState<string | null>(null);
  const [hasContent, setHasContent] = useState(false);
  const recoverableRef = useRef(true);
  const wsRunIdRef = useRef<string | null>(null);

  useEffect(() => {
    if (!runId) return;

    const isNewRun = wsRunIdRef.current !== runId;
    if (isNewRun) {
      wsRunIdRef.current = runId;
      recoverableRef.current = true;
      setInterrupted(null);
      setHasContent(false);
      dispatch({ type: 'reset', sessionId: runId });
    }

    setConnection('connecting');

    const protocol = window.location.protocol === 'https:' ? 'wss' : 'ws';
    const ws = new WebSocket(
      `${protocol}://${window.location.host}/ws/agent-runs/${runId}/live`,
    );

    ws.onopen = () => setConnection('open');
    ws.onclose = () => setConnection('closed');
    ws.onerror = () => setConnection('closed');
    ws.onmessage = (event) => {
      const msg = JSON.parse(event.data as string) as {
        type?: string;
        messages?: Message[];
        parts?: Record<string, Part[]>;
        sessionId?: string;
        event?: OpenCodeEvent;
        recoverable?: boolean;
        reason?: string | null;
      };

      if (msg.type === 'snapshot') {
        dispatch({
          type: 'snapshot',
          snapshot: {
            sessionId: msg.sessionId ?? runId,
            messages: msg.messages ?? [],
            parts: msg.parts ?? {},
          },
        });
        setHasContent(true);
      } else if (msg.type === 'event' && msg.event) {
        dispatch({ type: 'event', event: msg.event });
        setHasContent(true);
      } else if (msg.type === 'end') {
        if (shouldStopReconnect(msg)) {
          recoverableRef.current = false;
        }
        if (msg.reason) {
          setInterrupted(msg.reason);
        }
        ws.close();
      }
    };

    return () => ws.close();
  }, [runId, reconnectToken]);

  useEffect(() => {
    if (!isActiveRunStatus(runStatus) || connection !== 'closed' || !runId) {
      return;
    }
    if (!recoverableRef.current) return;

    const timer = window.setTimeout(() => {
      setReconnectToken((token) => token + 1);
    }, 800);
    return () => window.clearTimeout(timer);
  }, [runStatus, connection, runId]);

  if (!runId) {
    return (
      <p className="font-body text-sm text-text-muted">
        No agent run yet. Start a run to stream live output here.
      </p>
    );
  }

  const statusLabel = interrupted
    ? 'Interrupted'
    : connection === 'open'
      ? 'Live'
      : connection === 'connecting'
        ? 'Connecting…'
        : isActiveRunStatus(runStatus) && recoverableRef.current
          ? 'Disconnected — reconnecting…'
          : hasContent
            ? 'Finished'
            : 'Disconnected';

  return (
    <div className="flex h-full min-h-[320px] flex-col gap-2">
      <p className="font-body text-xs text-text-secondary">{statusLabel}</p>
      {interrupted && (
        <p className="font-body text-xs text-warning">{interrupted}</p>
      )}
      <div className="min-h-[280px] flex-1 overflow-y-auto rounded-md border border-border bg-surface px-3 py-2">
        {store ? (
          <SessionView store={store} />
        ) : (
          <p className="font-body text-sm text-text-muted">
            Waiting for session…
          </p>
        )}
      </div>
    </div>
  );
}
