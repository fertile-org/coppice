import type {
  CheckStatus,
  ConnectorCheck,
  ConnectorStatus,
} from '../../lib/schemas/connectorDiagnostics';
import { readinessLabel } from './connectorCopy';
import { CHECK_STATUS_LABELS } from './connectorFormat';

export type ConnectorFilter = 'all' | 'enabled' | 'disabled' | 'attention';

export type RowTone = 'danger' | 'warning' | 'muted' | 'info';

export interface RowSecondary {
  text: string;
  alert: boolean;
  tone: RowTone;
}

export function isActiveCheckStatus(status: CheckStatus | undefined): boolean {
  return status === 'queued' || status === 'running';
}

export function connectorNeedsAttention(connector: ConnectorStatus): boolean {
  if (!connector.enabled || !connector.probedAt) return false;
  if (
    connector.readiness === 'not_on_path' ||
    connector.readiness === 'found_not_signed_in'
  ) {
    return true;
  }
  const probe = connector.cli.probe.status;
  if (probe === 'failed' || probe === 'timed_out') return true;
  return connector.lastCheck?.status === 'failed';
}

export function attentionReason(connector: ConnectorStatus): string | null {
  if (!connectorNeedsAttention(connector)) return null;
  if (connector.readiness === 'not_on_path') return 'Not on your PATH';
  if (connector.readiness === 'found_not_signed_in') return 'Found, not signed in';
  if (connector.cli.probe.status === 'failed') return 'Probe failed';
  if (connector.cli.probe.status === 'timed_out') return 'Probe timed out';
  if (connector.lastCheck?.status === 'failed') {
    const failure = connector.lastCheck.failure?.trim();
    return failure ? `Last test failed: ${failure}` : 'Last test failed';
  }
  return null;
}

/** Prefer an in-flight lastCheck; otherwise a check id started this session. */
export function activeCheckId(
  connector: ConnectorStatus,
  sessionCheckId: string | null | undefined,
): string | null {
  if (connector.lastCheck && isActiveCheckStatus(connector.lastCheck.status)) {
    return connector.lastCheck.id;
  }
  return sessionCheckId ?? null;
}

export function liveCheckSecondary(
  check: ConnectorCheck | undefined,
  checkError: boolean,
): RowSecondary | null {
  if (checkError) {
    return { text: 'Unable to load test status.', alert: true, tone: 'danger' };
  }
  if (!check) return { text: 'Starting…', alert: false, tone: 'info' };
  if (!isActiveCheckStatus(check.status)) return null;
  return {
    text: CHECK_STATUS_LABELS[check.status],
    alert: false,
    tone: 'info',
  };
}

export function rowSecondaryText(
  connector: ConnectorStatus,
  toggleError: string | null,
  liveCheck: ConnectorCheck | undefined,
  checkError: boolean,
  hasLiveCheck: boolean,
): RowSecondary | null {
  if (toggleError) return { text: toggleError, alert: true, tone: 'danger' };
  if (hasLiveCheck) {
    const live = liveCheckSecondary(liveCheck, checkError);
    if (live) return live;
  }
  const reason = attentionReason(connector);
  if (reason) {
    const tone: RowTone =
      connector.lastCheck?.status === 'failed' &&
      connector.readiness !== 'not_on_path' &&
      connector.readiness !== 'found_not_signed_in' &&
      connector.cli.probe.status !== 'failed' &&
      connector.cli.probe.status !== 'timed_out'
        ? 'danger'
        : 'warning';
    return { text: reason, alert: false, tone };
  }
  if (!connector.probedAt) {
    return { text: 'Checking…', alert: false, tone: 'muted' };
  }
  const label = readinessLabel(connector.readiness);
  if (!label) return null;
  const version =
    connector.cli.probe.status === 'ok' && connector.cli.probe.detail
      ? connector.cli.probe.detail
      : null;
  return {
    text: version ? `${label} · ${version}` : label,
    alert: false,
    tone: 'muted',
  };
}

export function matchesSearch(connector: ConnectorStatus, query: string): boolean {
  const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (terms.length === 0) return true;
  const haystack = `${connector.displayName} ${connector.id}`.toLowerCase();
  return terms.every((term) => haystack.includes(term));
}

export function matchesFilter(
  connector: ConnectorStatus,
  filter: ConnectorFilter,
): boolean {
  switch (filter) {
    case 'all':
      return true;
    case 'enabled':
      return connector.enabled;
    case 'disabled':
      return !connector.enabled;
    case 'attention':
      return connectorNeedsAttention(connector);
  }
}

export function compareConnectors(a: ConnectorStatus, b: ConnectorStatus): number {
  const attn =
    Number(connectorNeedsAttention(b)) - Number(connectorNeedsAttention(a));
  if (attn !== 0) return attn;
  const enabled = Number(b.enabled) - Number(a.enabled);
  if (enabled !== 0) return enabled;
  return a.displayName.localeCompare(b.displayName);
}

export function visibleConnectors(
  connectors: ConnectorStatus[],
  query: string,
  filter: ConnectorFilter,
): { visible: ConnectorStatus[]; counts: Record<ConnectorFilter, number> } {
  const searched = connectors.filter((connector) => matchesSearch(connector, query));
  const counts = {
    all: searched.length,
    enabled: searched.filter((connector) => connector.enabled).length,
    disabled: searched.filter((connector) => !connector.enabled).length,
    attention: searched.filter(connectorNeedsAttention).length,
  };
  return {
    visible: searched
      .filter((connector) => matchesFilter(connector, filter))
      .sort(compareConnectors),
    counts,
  };
}

export function agentCountLabel(count: number | null): string {
  if (count === null) return '…';
  if (count === 0) return 'No agents';
  if (count === 1) return '1 agent';
  return `${count} agents`;
}

export function lastRunMeta(connector: ConnectorStatus, formatRelative: (iso: string) => string): string {
  if (!connector.lastRun) return 'no runs';
  return `ran ${formatRelative(connector.lastRun.finishedAt)}`;
}
