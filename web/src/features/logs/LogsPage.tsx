import { useQuery } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { Navigate, useSearchParams } from 'react-router-dom';
import { Button } from '../../components/ui/button';
import { Input } from '../../components/ui/input';
import { Label } from '../../components/ui/label';
import { apiFetch, parseApiErrorMessage } from '../../lib/api';
import { useSession } from '../auth/useSession';
import { LEVEL_OPTIONS, LOGS_COPY, SOURCE_OPTIONS, sourceLabel } from './copy';
import { filtersFromParams, fromUtcRfc3339, logsPath, toUtcRfc3339, type LogFilters } from './query';
import { logLineSchema, logPageSchema, type LogLine, type LogPage } from './schema';

async function fetchLogs(filters: LogFilters, cursor: string | null): Promise<LogPage> {
  const response = await apiFetch(logsPath(filters, cursor));
  return logPageSchema.parse(await response.json());
}

function lineKey(line: LogLine): string {
  return `${line.timestamp ?? ''}|${line.source}|${line.runId ?? ''}|${line.message}`;
}

function fieldText(line: LogLine): string {
  const method = typeof line.fields.method === 'string' ? line.fields.method : '';
  const path = typeof line.fields.path === 'string' ? line.fields.path : '';
  const status = line.fields.status;
  const statusText =
    typeof status === 'number' || typeof status === 'string' ? String(status) : '';
  return [method, path, statusText].filter(Boolean).join(' ');
}

function LogRow({ line }: { line: LogLine }) {
  const extra = fieldText(line);
  return (
    <li className="border-b border-border py-2 font-body text-xs text-text-secondary">
      <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <time className="font-mono text-text-muted">{line.timestamp ?? ''}</time>
        <span>{line.level}</span>
        <span className="text-text-primary">{sourceLabel(line.source)}</span>
        {line.stream && <span className="font-mono">{line.stream}</span>}
        {line.runId && <span className="font-mono text-text-muted">{line.runId}</span>}
      </div>
      <p className="mt-1 whitespace-pre-wrap font-mono text-text-primary">{line.message}</p>
      {extra && <p className="mt-1 font-mono text-text-muted">{extra}</p>}
    </li>
  );
}

export function LogsPage() {
  const { user, loading } = useSession();
  const [params, setParams] = useSearchParams();
  const filters = filtersFromParams(params);
  const filterKey = logsPath(filters, null);
  const [live, setLive] = useState(false);
  const [liveLines, setLiveLines] = useState<{ key: string; lines: LogLine[] }>({
    key: filterKey,
    lines: [],
  });
  const [older, setOlder] = useState<{ key: string; lines: LogLine[]; cursor: string | null }>({
    key: '',
    lines: [],
    cursor: null,
  });
  const [loadingOlder, setLoadingOlder] = useState(false);

  const logsQuery = useQuery({
    queryKey: ['logs', filterKey],
    queryFn: () => fetchLogs(filters, null),
    enabled: user?.role === 'admin',
  });

  useEffect(() => {
    if (!live || user?.role !== 'admin') {
      return;
    }
    const protocol = window.location.protocol === 'https:' ? 'wss' : 'ws';
    const socket = new WebSocket(`${protocol}://${window.location.host}/ws/logs`);
    socket.onopen = () => {
      socket.send(
        JSON.stringify({
          source: filters.source || undefined,
          level: filters.level || 'info',
          from: filters.from || undefined,
          to: filters.to || undefined,
          run: filters.run || undefined,
          q: filters.q || undefined,
        }),
      );
    };
    socket.onmessage = (event) => {
      const parsed = logLineSchema.safeParse(JSON.parse(String(event.data)));
      if (!parsed.success) {
        return;
      }
      const line = parsed.data;
      setLiveLines((current) => {
        const lines = current.key === filterKey ? current.lines : [];
        if (lines.some((existing) => lineKey(existing) === lineKey(line))) {
          return { key: filterKey, lines };
        }
        return { key: filterKey, lines: [line, ...lines] };
      });
    };
    return () => {
      socket.close();
    };
  }, [
    live,
    filterKey,
    filters.source,
    filters.level,
    filters.from,
    filters.to,
    filters.run,
    filters.q,
    user?.role,
  ]);

  if (loading) {
    return <p className="font-body text-sm text-text-muted">{LOGS_COPY.loading}</p>;
  }
  if (user?.role !== 'admin') {
    return <Navigate to="/boards" replace />;
  }

  function setFilter(key: keyof LogFilters, value: string) {
    const next = new URLSearchParams(params);
    if (!value || (key === 'level' && value === 'info')) {
      next.delete(key);
    } else {
      next.set(key, value);
    }
    setParams(next, { replace: true });
  }

  const history = logsQuery.data?.lines ?? [];
  const prepended = liveLines.key === filterKey ? liveLines.lines : [];
  const appended = older.key === filterKey ? older.lines : [];
  const lines = [...prepended, ...history, ...appended];
  const nextCursor =
    older.key === filterKey && older.lines.length > 0
      ? older.cursor
      : (logsQuery.data?.nextCursor ?? null);

  async function loadOlder() {
    if (!nextCursor) {
      return;
    }
    setLoadingOlder(true);
    try {
      const page = await fetchLogs(filters, nextCursor);
      setOlder({
        key: filterKey,
        lines: [...appended, ...page.lines],
        cursor: page.nextCursor,
      });
    } finally {
      setLoadingOlder(false);
    }
  }

  return (
    <div className="space-y-6">
      <h1 className="font-display text-2xl font-semibold text-text-primary">{LOGS_COPY.title}</h1>
      <div className="flex flex-wrap items-end gap-4">
        <div className="space-y-1">
          <Label htmlFor="logs-source">{LOGS_COPY.source}</Label>
          <select
            id="logs-source"
            className="field-control px-3 py-2 font-body text-sm"
            value={filters.source}
            onChange={(event) => setFilter('source', event.target.value)}
          >
            {SOURCE_OPTIONS.map((option) => (
              <option key={option.label} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </div>
        <div className="space-y-1">
          <Label htmlFor="logs-level">{LOGS_COPY.level}</Label>
          <select
            id="logs-level"
            className="field-control px-3 py-2 font-body text-sm"
            value={filters.level}
            onChange={(event) => setFilter('level', event.target.value)}
          >
            {LEVEL_OPTIONS.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </div>
        <fieldset className="space-y-1">
          <legend className="font-body text-sm font-medium text-text-secondary">
            {LOGS_COPY.time}
          </legend>
          <div className="flex flex-wrap gap-2">
            <Label className="space-y-1">
              <span>{LOGS_COPY.from}</span>
              <Input
                type="datetime-local"
                value={fromUtcRfc3339(filters.from)}
                onChange={(event) => setFilter('from', toUtcRfc3339(event.target.value))}
              />
            </Label>
            <Label className="space-y-1">
              <span>{LOGS_COPY.to}</span>
              <Input
                type="datetime-local"
                value={fromUtcRfc3339(filters.to)}
                onChange={(event) => setFilter('to', toUtcRfc3339(event.target.value))}
              />
            </Label>
          </div>
        </fieldset>
        <div className="space-y-1">
          <Label htmlFor="logs-run">{LOGS_COPY.run}</Label>
          <Input
            id="logs-run"
            value={filters.run}
            onChange={(event) => setFilter('run', event.target.value)}
          />
        </div>
        <div className="min-w-48 flex-1 space-y-1">
          <Label htmlFor="logs-search">{LOGS_COPY.search}</Label>
          <Input
            id="logs-search"
            placeholder={LOGS_COPY.search}
            value={filters.q}
            onChange={(event) => setFilter('q', event.target.value)}
          />
        </div>
        <label className="flex items-center gap-2 pb-2 font-body text-sm text-text-secondary">
          <input
            type="checkbox"
            checked={live}
            onChange={(event) => setLive(event.target.checked)}
          />
          {LOGS_COPY.live}
        </label>
      </div>

      {logsQuery.isError && (
        <p className="font-body text-sm text-danger">
          {parseApiErrorMessage(logsQuery.error, LOGS_COPY.loadError)}
        </p>
      )}
      {logsQuery.isLoading && (
        <p className="font-body text-sm text-text-muted">{LOGS_COPY.loading}</p>
      )}
      {!logsQuery.isLoading && !logsQuery.isError && lines.length === 0 && (
        <p className="font-body text-sm text-text-muted">{LOGS_COPY.empty}</p>
      )}
      {lines.length > 0 && (
        <ul aria-label={LOGS_COPY.title}>
          {lines.map((line, index) => (
            <LogRow key={`${lineKey(line)}-${index}`} line={line} />
          ))}
        </ul>
      )}
      {nextCursor && (
        <Button type="button" variant="secondary" onClick={() => void loadOlder()} disabled={loadingOlder}>
          {LOGS_COPY.loadOlder}
        </Button>
      )}
    </div>
  );
}
