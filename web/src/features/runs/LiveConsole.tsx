import { FitAddon } from '@xterm/addon-fit';
import { Terminal } from 'xterm';
import { useEffect, useRef, useState } from 'react';
import 'xterm/css/xterm.css';
import '../../opencode-session/theme/opencode-theme.css';
import { openCodeXtermTheme } from '../../opencode-session/theme/ayu-palette';
import { sessionTheme } from '../../opencode-session/theme/session-theme';
import { LiveRunActivityBar } from './LiveRunActivityBar';

interface LiveConsoleProps {
  runId: string | null;
  runStatus: string | null;
  startedAt?: string | null;
}

type ConnectionState = 'connecting' | 'open' | 'closed';

function isActiveRunStatus(status: string | null): boolean {
  return status === 'running' || status === 'queued';
}

/** xterm needs CRLF; bare `\n` advances the row without resetting the column. */
function writeTerminalData(term: Terminal, data: string) {
  term.write(data.replace(/\r?\n/g, '\r\n'));
}

export function LiveConsole({ runId, runStatus, startedAt }: LiveConsoleProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const [termReady, setTermReady] = useState(false);
  const [connection, setConnection] = useState<ConnectionState>('closed');
  const [reconnectToken, setReconnectToken] = useState(0);
  const [sawOutput, setSawOutput] = useState(false);
  const [lastActivityAt, setLastActivityAt] = useState<number | null>(null);
  const [heartbeatElapsedSecs, setHeartbeatElapsedSecs] = useState<number | null>(
    null,
  );
  const sawOutputRef = useRef(false);
  const wsRunIdRef = useRef<string | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;
    const term = new Terminal({
      theme: openCodeXtermTheme,
      fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
      fontSize: 15,
      scrollback: 5000,
      convertEol: true,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(containerRef.current);
    fit.fit();
    termRef.current = term;
    fitRef.current = fit;
    setTermReady(true);
    return () => {
      term.dispose();
      termRef.current = null;
      fitRef.current = null;
      setTermReady(false);
    };
  }, []);

  useEffect(() => {
    if (!runId || !termReady || !termRef.current) return;

    const isNewRun = wsRunIdRef.current !== runId;
    if (isNewRun) {
      termRef.current.clear();
      wsRunIdRef.current = runId;
      sawOutputRef.current = false;
      setSawOutput(false);
      setLastActivityAt(null);
      setHeartbeatElapsedSecs(null);
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
        data?: string;
        status?: string;
        recoverable?: boolean;
        sessionStatus?: string;
        elapsedSecs?: number;
      };
      setLastActivityAt(Date.now());
      if (msg.type === 'heartbeat') {
        if (typeof msg.sessionStatus === 'string') {
          // terminal view has no session UI; elapsed still useful
        }
        if (typeof msg.elapsedSecs === 'number') {
          setHeartbeatElapsedSecs(msg.elapsedSecs);
        }
        return;
      }
      if (msg.type === 'frame' && typeof msg.data === 'string') {
        if (termRef.current) writeTerminalData(termRef.current, msg.data);
        sawOutputRef.current = true;
        setSawOutput(true);
      }
      if (msg.type === 'end') {
        const shouldReconnect =
          !sawOutputRef.current &&
          msg.status &&
          isActiveRunStatus(msg.status) &&
          msg.recoverable !== false;
        if (shouldReconnect) {
          // Stream not ready yet; reconnect will pick up live frames.
          ws.close();
          return;
        }
        ws.close();
      }
    };

    return () => ws.close();
  }, [runId, reconnectToken, termReady]);

  useEffect(() => {
    if (!isActiveRunStatus(runStatus) || connection !== 'closed' || !runId) return;
    const timer = window.setTimeout(() => {
      setReconnectToken((token) => token + 1);
    }, 800);
    return () => window.clearTimeout(timer);
  }, [runStatus, connection, runId]);

  useEffect(() => {
    if (!containerRef.current || !fitRef.current) return;
    const observer = new ResizeObserver(() => fitRef.current?.fit());
    observer.observe(containerRef.current);
    return () => observer.disconnect();
  }, [termReady]);

  if (!runId) {
    return (
      <p className="font-body text-sm text-text-muted">
        No agent run yet. Start a run to stream live output here.
      </p>
    );
  }

  const statusLabel =
    connection === 'open'
      ? 'Live'
      : connection === 'connecting'
        ? 'Connecting…'
        : isActiveRunStatus(runStatus)
          ? 'Disconnected — reconnecting…'
          : sawOutput
            ? 'Finished'
            : 'Disconnected';

  return (
    <div className="flex h-full min-h-[320px] flex-col gap-2">
      <LiveRunActivityBar
        runStatus={runStatus}
        startedAt={startedAt}
        connection={connection}
        lastActivityAt={lastActivityAt}
        heartbeatElapsedSecs={heartbeatElapsedSecs}
      />
      <p className="font-body text-xs text-text-secondary">{statusLabel}</p>
      <div
        ref={containerRef}
        className={`oc-session min-h-[280px] flex-1 overflow-hidden border border-[var(--oc-border)] p-1 ${sessionTheme.bg}`}
      />
    </div>
  );
}
