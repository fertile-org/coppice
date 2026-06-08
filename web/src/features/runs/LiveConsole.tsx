import { FitAddon } from '@xterm/addon-fit';
import { Terminal } from 'xterm';
import { useEffect, useRef, useState } from 'react';
import 'xterm/css/xterm.css';

interface LiveConsoleProps {
  runId: string | null;
  runStatus: string | null;
}

type ConnectionState = 'connecting' | 'open' | 'closed';

export function LiveConsole({ runId, runStatus }: LiveConsoleProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const [connection, setConnection] = useState<ConnectionState>('closed');
  const [reconnectToken, setReconnectToken] = useState(0);

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
    return () => {
      term.dispose();
      termRef.current = null;
      fitRef.current = null;
    };
  }, []);

  useEffect(() => {
    if (!runId || !termRef.current) return;
    termRef.current.clear();
    const protocol = window.location.protocol === 'https:' ? 'wss' : 'ws';
    const ws = new WebSocket(
      `${protocol}://${window.location.host}/ws/agent-runs/${runId}/live`,
    );
    setConnection('connecting');

    ws.onopen = () => setConnection('open');
    ws.onclose = () => setConnection('closed');
    ws.onerror = () => setConnection('closed');
    ws.onmessage = (event) => {
      const msg = JSON.parse(event.data as string) as {
        type?: string;
        data?: string;
      };
      if (msg.type === 'frame' && typeof msg.data === 'string') {
        termRef.current?.write(msg.data);
      }
      if (msg.type === 'end') {
        ws.close();
      }
    };

    return () => ws.close();
  }, [runId, reconnectToken]);

  useEffect(() => {
    if (runStatus !== 'running' || connection !== 'closed' || !runId) return;
    const timer = window.setTimeout(() => {
      setReconnectToken((token) => token + 1);
    }, 2000);
    return () => window.clearTimeout(timer);
  }, [runStatus, connection, runId]);

  useEffect(() => {
    if (!containerRef.current || !fitRef.current) return;
    const observer = new ResizeObserver(() => fitRef.current?.fit());
    observer.observe(containerRef.current);
    return () => observer.disconnect();
  }, []);

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
        : runStatus === 'running'
          ? 'Disconnected — reconnecting…'
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
