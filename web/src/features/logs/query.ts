import { LOGS_COPY, reportProblemBody } from './copy';

export interface LogFilters {
  source: string;
  level: string;
  from: string;
  to: string;
  run: string;
  q: string;
}

export function toUtcRfc3339(value: string): string {
  const trimmed = value.trim();
  if (!trimmed) {
    return '';
  }
  if (trimmed.endsWith('Z') || /[+-]\d{2}:\d{2}$/.test(trimmed)) {
    return trimmed;
  }
  const withSeconds = trimmed.length === 16 ? `${trimmed}:00` : trimmed;
  return `${withSeconds}Z`;
}

/** `datetime-local` value for a UTC timestamp. */
export function fromUtcRfc3339(value: string): string {
  if (!value) {
    return '';
  }
  const noZone = value.replace(/Z$/, '').replace(/[+-]\d{2}:\d{2}$/, '');
  const noFraction = noZone.replace(/\.\d+$/, '');
  return noFraction.slice(0, 16);
}

export function logsPath(filters: LogFilters, cursor: string | null): string {
  const params = new URLSearchParams();
  if (filters.source) {
    params.set('source', filters.source);
  }
  params.set('level', filters.level || 'info');
  if (filters.from) {
    params.set('from', filters.from);
  }
  if (filters.to) {
    params.set('to', filters.to);
  }
  if (filters.run) {
    params.set('run', filters.run);
  }
  if (filters.q) {
    params.set('q', filters.q);
  }
  if (cursor) {
    params.set('cursor', cursor);
  }
  return `/api/logs?${params.toString()}`;
}

export function exportPayload(filters: LogFilters, shellVersion: string): Record<string, string> {
  const body: Record<string, string> = {
    level: filters.level || 'info',
  };
  if (filters.source) {
    body.source = filters.source;
  }
  if (filters.from) {
    body.from = filters.from;
  }
  if (filters.to) {
    body.to = filters.to;
  }
  if (filters.run) {
    body.run = filters.run;
  }
  if (filters.q) {
    body.q = filters.q;
  }
  const shell = shellVersion.trim();
  if (shell) {
    body.shellVersion = shell;
  }
  return body;
}

export function filenameFromDisposition(header: string | null): string {
  const match = /filename="([^"]+)"/.exec(header ?? '');
  return match?.[1] || LOGS_COPY.exportFile;
}

export function reportProblemUrl(version: string, osVersion: string): string {
  const url = new URL('https://github.com/fertile-org/coppice/issues/new');
  url.searchParams.set('body', reportProblemBody(version, osVersion));
  return url.toString();
}

export function filtersFromParams(params: URLSearchParams): LogFilters {
  return {
    source: params.get('source') ?? '',
    level: params.get('level') ?? 'info',
    from: params.get('from') ?? '',
    to: params.get('to') ?? '',
    run: params.get('run') ?? '',
    q: params.get('q') ?? '',
  };
}
