import type { CheckStatus } from '../../lib/schemas/connectorDiagnostics';

export const CHECK_STATUS_LABELS: Record<CheckStatus, string> = {
  queued: 'Queued…',
  running: 'Running…',
  passed: 'Passed',
  failed: 'Failed',
};

export function checkStatusClass(status: CheckStatus): string {
  switch (status) {
    case 'passed':
      return 'text-success';
    case 'failed':
      return 'text-danger';
    default:
      return 'text-info';
  }
}

export function formatRelativeTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  const diffMs = date.getTime() - Date.now();
  const absSec = Math.round(Math.abs(diffMs) / 1000);
  const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });
  if (absSec < 60) return rtf.format(Math.round(diffMs / 1000), 'second');
  if (absSec < 3600) return rtf.format(Math.round(diffMs / 60000), 'minute');
  if (absSec < 86400) return rtf.format(Math.round(diffMs / 3600000), 'hour');
  return rtf.format(Math.round(diffMs / 86400000), 'day');
}
