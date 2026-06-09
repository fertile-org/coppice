import { FitAddon } from '@xterm/addon-fit';
import { Terminal } from 'xterm';
import { useEffect, useRef, useState } from 'react';
import 'xterm/css/xterm.css';

interface LiveConsoleProps {
  runId: string | null;
  runStatus: string | null;
}

type ConnectionState = 'connecting' | 'open' | 'closed';

function isActiveRunStatus(status: string | null): boolean {
  return status === 'running' || status === 'queued';
}

/** xterm needs CRLF; bare `\n` advances the row without resetting the column. */
function writeTerminalData(term: Terminal, data: string) {
  term.write(data.replace(/\r?\n/g, '\r\n'));
}

export function LiveConsole({ runId, runStatus }: LiveConsoleProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const [termReady, setTermReady] = useState(false);
  const [connection, setConnection] = useState<ConnectionState>('closed');
  const [reconnectToken, setReconnectToken] = useState(0);
  const [sawOutput, setSawOutput] = useState(false);
  const sawOutputRef = useRef(false);
  const wsRunIdRef = useRef<string | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;
    const term = new Terminal({
      fontFamily: 'ui-monospace, monospace',
      fontSize: 13,
      scrollback: 5000,
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
      };
      if (msg.type === 'frame' && typeof msg.data === 'string') {
        if (termRef.current) writeTerminalData(termRef.current, msg.data);
        sawOutputRef.current = true;
        setSawOutput(true);
      }
      if (msg.type === 'end') {
        if (
          !sawOutputRef.current &&
          msg.status &&
          isActiveRunStatus(msg.status)
        ) {
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
      <p className="font-body text-xs text-text-secondary">{statusLabel}</p>
      <div
        ref={containerRef}
        className="min-h-[280px] flex-1 overflow-hidden rounded-md border border-border bg-[#1e1e1e] p-1"
      />
    </div>
  );
}
