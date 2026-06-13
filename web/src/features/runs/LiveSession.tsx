import { useEffect, useReducer, useRef, useState } from 'react';
import '../../opencode-session/theme/opencode-theme.css';
import { SessionView } from '../../opencode-session/session/SessionView';
import { sessionTheme } from '../../opencode-session/theme/session-theme';
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
import { LiveRunActivityBar } from './LiveRunActivityBar';

interface LiveSessionProps {
  runId: string | null;
  runStatus: string | null;
  startedAt?: string | null;
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

function sessionStatusFromEvent(event: OpenCodeEvent): string | null {
  if (event.type !== 'session.status') return null;
  const props = event.properties;
  if (!props || typeof props !== 'object') return null;
  const status = (props as { status?: { type?: string } }).status;
  return typeof status?.type === 'string' ? status.type : null;
}

export function LiveSession({ runId, runStatus, startedAt }: LiveSessionProps) {
  const [store, dispatch] = useReducer(sessionReducer, null);
  const [connection, setConnection] = useState<ConnectionState>('closed');
  const [reconnectToken, setReconnectToken] = useState(0);
  const [interrupted, setInterrupted] = useState<string | null>(null);
  const [hasContent, setHasContent] = useState(false);
  const [lastActivityAt, setLastActivityAt] = useState<number | null>(null);
  const [sessionStatus, setSessionStatus] = useState<string | null>(null);
  const [heartbeatElapsedSecs, setHeartbeatElapsedSecs] = useState<number | null>(
    null,
  );
  const hasContentRef = useRef(false);
  const recoverableRef = useRef(true);

  function touchActivity() {
    setLastActivityAt(Date.now());
  }

  // Initialize store when runId changes (also recovers after React StrictMode remount).
  useEffect(() => {
    if (!runId) return;
    recoverableRef.current = true;
    setInterrupted(null);
    setHasContent(false);
    hasContentRef.current = false;
    setLastActivityAt(null);
    setSessionStatus(null);
    setHeartbeatElapsedSecs(null);
    dispatch({ type: 'reset', sessionId: runId });
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
        event?: OpenCodeEvent;
        recoverable?: boolean;
        reason?: string | null;
        status?: string;
        sessionStatus?: string;
        elapsedSecs?: number;
      };

      touchActivity();

      if (msg.type === 'heartbeat') {
        if (typeof msg.sessionStatus === 'string') {
          setSessionStatus(msg.sessionStatus);
        }
        if (typeof msg.elapsedSecs === 'number') {
          setHeartbeatElapsedSecs(msg.elapsedSecs);
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
        if (msg.reason) {
          setInterrupted(msg.reason);
        }
        if (
          !hasContentRef.current &&
          msg.status &&
          isActiveRunStatus(msg.status) &&
          recoverableRef.current
        ) {
          // Stream not ready yet; reconnect will pick up live frames.
          ws.close();
          return;
        }
        ws.close();
      }
    };

    return () => {
      active = false;
      ws.close();
    };
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
      <LiveRunActivityBar
        runStatus={runStatus}
        startedAt={startedAt}
        connection={connection}
        sessionStatus={sessionStatus}
        lastActivityAt={lastActivityAt}
        heartbeatElapsedSecs={heartbeatElapsedSecs}
      />
      <p className="font-body text-xs text-text-secondary">{statusLabel}</p>
      {interrupted && (
        <p className="font-body text-xs text-warning">{interrupted}</p>
      )}
      <div
        className={`oc-session min-h-[280px] flex-1 overflow-y-auto border border-[var(--oc-border)] px-4 py-4 ${sessionTheme.bg}`}
      >
        {store ? (
          <SessionView store={store} />
        ) : (
          <p className={`${sessionTheme.fontBody} ${sessionTheme.textMuted}`}>
            Waiting for session…
          </p>
        )}
      </div>
    </div>
  );
}
