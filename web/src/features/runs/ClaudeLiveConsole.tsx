import { useEffect, useReducer, useRef, useState } from 'react';
import '../../opencode-session/theme/opencode-theme.css';
import { sessionTheme } from '../../opencode-session/theme/session-theme';
import { ClaudeConsoleView } from './ClaudeConsoleView';
import {
  appendLegacyFrameText,
  applyClaudeConsoleEvent,
  createClaudeConsoleState,
  resetClaudeConsoleState,
  type ClaudeConsoleState,
} from './claude-console-state';
import { LiveRunActivityBar } from './LiveRunActivityBar';

interface ClaudeLiveConsoleProps {
  runId: string | null;
  runStatus: string | null;
  shouldReconnect?: boolean;
  startedAt?: string | null;
}

type ConnectionState = 'connecting' | 'open' | 'closed' | 'reconnecting';

type ConsoleAction =
  | { type: 'reset' }
  | { type: 'event'; event: Record<string, unknown> }
  | { type: 'frame'; text: string };

function consoleReducer(
  state: ClaudeConsoleState,
  action: ConsoleAction,
): ClaudeConsoleState {
  switch (action.type) {
    case 'reset':
      return resetClaudeConsoleState();
    case 'event':
      return applyClaudeConsoleEvent(state, action.event);
    case 'frame':
      return appendLegacyFrameText(state, action.text);
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

function isStructuredConsoleEvent(event: Record<string, unknown>): boolean {
  const ty = event.type;
  return typeof ty === 'string' && ty.includes('.console.');
}

export function ClaudeLiveConsole({
  runId,
  runStatus,
  shouldReconnect,
  startedAt,
}: ClaudeLiveConsoleProps) {
  const [state, dispatch] = useReducer(consoleReducer, null, createClaudeConsoleState);
  const [connection, setConnection] = useState<ConnectionState>('closed');
  const [reconnectToken, setReconnectToken] = useState(0);
  const [interrupted, setInterrupted] = useState<string | null>(null);
  const [lastActivityAt, setLastActivityAt] = useState<number | null>(null);
  const hasContentRef = useRef(false);
  const recoverableRef = useRef(true);

  function touchActivity() {
    setLastActivityAt(Date.now());
  }

  useEffect(() => {
    if (!runId) return;
    recoverableRef.current = true;
    setInterrupted(null);
    hasContentRef.current = false;
    setLastActivityAt(null);
    dispatch({ type: 'reset' });
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
        event?: Record<string, unknown>;
        data?: string;
        recoverable?: boolean;
        reason?: string | null;
        status?: string;
      };

      touchActivity();

      if (msg.type === 'heartbeat') {
        return;
      }

      if (msg.type === 'event' && msg.event && isStructuredConsoleEvent(msg.event)) {
        dispatch({ type: 'event', event: msg.event });
        hasContentRef.current = true;
      } else if (msg.type === 'frame' && typeof msg.data === 'string') {
        dispatch({ type: 'frame', text: msg.data });
        hasContentRef.current = true;
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
    const canReconnect = shouldReconnect ?? isActiveRunStatus(runStatus);
    if (!canReconnect || connection !== 'closed' || !runId) {
      return;
    }
    if (!recoverableRef.current) return;

    const timer = window.setTimeout(() => {
      setReconnectToken((token) => token + 1);
    }, 800);
    return () => window.clearTimeout(timer);
  }, [runStatus, shouldReconnect, connection, runId]);

  if (!runId) {
    return (
      <p className="font-body text-sm text-text-muted">
        No agent run yet. Start a run to stream live output here.
      </p>
    );
  }

  const hasContent =
    hasContentRef.current ||
    state.entries.length > 0 ||
    state.legacyText.trim().length > 0;

  const canReconnect =
    (shouldReconnect ?? isActiveRunStatus(runStatus)) && recoverableRef.current;
  const displayConnection =
    connection === 'closed' && canReconnect ? 'reconnecting' : connection;
  const statusLabel = interrupted
    ? 'Interrupted'
    : displayConnection === 'open'
      ? 'Live'
      : displayConnection === 'connecting'
        ? 'Connecting…'
        : displayConnection === 'reconnecting'
          ? 'Disconnected — reconnecting…'
          : canReconnect
            ? 'Disconnected — reconnecting…'
            : hasContent
              ? 'Finished'
              : 'Disconnected';

  return (
    <div className="flex h-full min-h-[320px] flex-col gap-2">
      <LiveRunActivityBar
        runStatus={runStatus}
        startedAt={startedAt}
        shouldReconnect={shouldReconnect}
        connection={displayConnection}
        sessionStatus={null}
        lastActivityAt={lastActivityAt}
        heartbeatElapsedSecs={null}
      />
      <p className="font-body text-xs text-text-secondary">{statusLabel}</p>
      {interrupted ? (
        <p className="font-body text-xs text-warning">{interrupted}</p>
      ) : null}
      <div
        className={`oc-session min-h-[280px] flex-1 overflow-y-auto border border-[var(--oc-border)] px-4 py-4 ${sessionTheme.bg}`}
      >
        <ClaudeConsoleView
          entries={state.entries}
          legacyText={state.legacyText}
          isLive={canReconnect && connection === 'open'}
        />
      </div>
    </div>
  );
}
