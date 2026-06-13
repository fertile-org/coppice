import { useEffect, useState } from 'react';
import { isActiveRunStatus } from '../tickets/useAgentRuns';
import type { RunStatus } from '../../lib/schemas/agentRun';

function formatElapsed(totalSecs: number): string {
  if (totalSecs < 60) return `${totalSecs}s`;
  const mins = Math.floor(totalSecs / 60);
  const secs = totalSecs % 60;
  if (mins < 60) return secs > 0 ? `${mins}m ${secs}s` : `${mins}m`;
  const hours = Math.floor(mins / 60);
  const remMins = mins % 60;
  return remMins > 0 ? `${hours}h ${remMins}m` : `${hours}h`;
}

function useNowTick(active: boolean, intervalMs = 1000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    const id = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => window.clearInterval(id);
  }, [active, intervalMs]);
  return now;
}

export interface LiveRunActivityBarProps {
  runStatus: string | null;
  startedAt?: string | null;
  connection: 'connecting' | 'open' | 'closed';
  sessionStatus?: string | null;
  lastActivityAt?: number | null;
  heartbeatElapsedSecs?: number | null;
}

export function LiveRunActivityBar({
  runStatus,
  startedAt,
  connection,
  sessionStatus,
  lastActivityAt,
  heartbeatElapsedSecs,
}: LiveRunActivityBarProps) {
  const active = isActiveRunStatus(runStatus as RunStatus | null);
  const now = useNowTick(active);

  if (!active) return null;

  const startedMs = startedAt ? Date.parse(startedAt) : NaN;
  const elapsedSecs = Number.isFinite(startedMs)
    ? Math.max(0, Math.floor((now - startedMs) / 1000))
    : (heartbeatElapsedSecs ?? 0);

  const quietSecs =
    lastActivityAt != null
      ? Math.max(0, Math.floor((now - lastActivityAt) / 1000))
      : null;

  const working =
    sessionStatus === 'busy' ||
    sessionStatus === 'running' ||
    (quietSecs != null && quietSecs < 45);

  let detail = 'Agent run in progress';
  if (sessionStatus === 'busy') {
    detail = 'Agent is working…';
  } else if (quietSecs != null && quietSecs >= 45) {
    detail = `No new output for ${formatElapsed(quietSecs)} — session may still be running`;
  } else if (connection === 'connecting') {
    detail = 'Connecting to live stream…';
  } else if (connection === 'closed') {
    detail = 'Reconnecting to live stream…';
  }

  return (
    <div
      className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-md border border-moss-500/30 bg-moss-500/10 px-3 py-2"
      role="status"
      aria-live="polite"
    >
      <span className="inline-flex items-center gap-2 font-body text-xs font-medium text-moss-700">
        <span className="relative flex h-2.5 w-2.5">
          <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-moss-500 opacity-60" />
          <span className="relative inline-flex h-2.5 w-2.5 rounded-full bg-moss-600" />
        </span>
        Running · {formatElapsed(elapsedSecs)}
      </span>
      <span className="font-body text-xs text-text-secondary">{detail}</span>
      {working && (
        <span className="inline-flex items-center gap-1" aria-hidden>
          {[0, 150, 300].map((delay) => (
            <span
              key={delay}
              className="inline-block h-1 w-1 animate-bounce rounded-full bg-text-muted"
              style={{ animationDelay: `${delay}ms` }}
            />
          ))}
        </span>
      )}
    </div>
  );
}
