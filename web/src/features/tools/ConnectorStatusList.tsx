import { useState, type ReactNode } from 'react';
import type { ConnectorStatus, LastRun } from '../../lib/schemas/connectorDiagnostics';
import { useOpenTicket } from '../tickets/useOpenTicket';
import { CHECK_STATUS_LABELS, checkStatusClass, formatRelativeTime } from './connectorFormat';

const CHECKING = <span className="text-text-muted">Checking…</span>;

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex gap-3">
      <dt className="w-24 shrink-0 text-text-muted">{label}</dt>
      <dd className="min-w-0 flex-1 space-y-0.5 text-text-primary">{children}</dd>
    </div>
  );
}

function CliValue({ connector }: { connector: ConnectorStatus }) {
  if (!connector.probedAt) return CHECKING;
  if (!connector.cli.found) return <span className="text-warning">Not installed</span>;
  return (
    <>
      <span className="text-success">Found</span>
      {connector.cli.path && (
        <p className="truncate font-mono text-xs" title={connector.cli.path}>
          {connector.cli.path}
        </p>
      )}
    </>
  );
}

function ProbeValue({ probe }: { probe: ConnectorStatus['cli']['probe'] }) {
  switch (probe.status) {
    case 'ok':
      return <p className="truncate font-mono text-xs">{probe.detail}</p>;
    case 'failed':
      return (
        <>
          <span className="text-danger">Failed</span>
          {probe.detail && (
            <pre className="mt-1 whitespace-pre-wrap rounded-md border border-danger-muted bg-danger-muted/30 p-2 font-mono text-xs text-danger">
              {probe.detail}
            </pre>
          )}
        </>
      );
    case 'timed_out':
      return <span className="text-danger">Timed out</span>;
    case 'not_run':
      return <span className="text-text-muted">Not run</span>;
  }
}

function AuthValue({ connector }: { connector: ConnectorStatus }) {
  if (!connector.probedAt) return CHECKING;
  const { status, envSet, pathsFound } = connector.auth;
  switch (status) {
    case 'detected':
      return (
        <>
          <span className="text-success">Detected</span>
          {envSet.length > 0 && (
            <p className="font-mono text-xs">{`Env: ${envSet.join(', ')}`}</p>
          )}
          {pathsFound.length > 0 && (
            <p className="font-mono text-xs">{`Files: ${pathsFound.join(', ')}`}</p>
          )}
        </>
      );
    case 'verified_by_probe':
      return <span className="text-success">Verified by probe</span>;
    case 'not_found':
      return <span className="text-warning">Not found</span>;
  }
}

function toolMark(ok: boolean): string {
  return ok ? '✓' : '✗';
}

function LastRunValue({ run }: { run: LastRun | null | undefined }) {
  const openTicket = useOpenTicket();
  const [error, setError] = useState<string | null>(null);
  if (!run) return <span className="text-text-muted">No runs yet</span>;
  const ticketId = run.ticketId;
  return (
    <>
      <p className="flex flex-wrap items-center gap-x-2">
        <span title={run.finishedAt}>{formatRelativeTime(run.finishedAt)}</span>
        <span className="capitalize text-text-secondary">{run.status}</span>
        {ticketId && (
          <button
            type="button"
            onClick={() => {
              setError(null);
              openTicket(ticketId).catch(() => setError('Unable to open ticket.'));
            }}
            className="font-medium text-moss-700 underline-offset-2 hover:underline"
          >
            Open ticket
          </button>
        )}
      </p>
      <p className="font-mono text-xs">
        {`ticket_get ${toolMark(run.ticketGet)} · result_submit ${toolMark(run.resultSubmit)}`}
      </p>
      {error && <p className="text-danger">{error}</p>}
    </>
  );
}

function LastTestValue({ check }: { check: ConnectorStatus['lastCheck'] }) {
  if (!check) return <span className="text-text-muted">Never</span>;
  return (
    <>
      <p className="flex flex-wrap items-center gap-x-2">
        <span className={checkStatusClass(check.status)}>
          {CHECK_STATUS_LABELS[check.status]}
        </span>
        <span title={check.createdAt} className="text-text-secondary">
          {formatRelativeTime(check.createdAt)}
        </span>
      </p>
      {check.failure && <p className="text-danger">{check.failure}</p>}
    </>
  );
}

export function ConnectorStatusList({ connector }: { connector: ConnectorStatus }) {
  const showProbe = Boolean(connector.probedAt) && connector.cli.found;
  return (
    <dl className="mt-3 space-y-2 font-body text-xs">
      <Row label="CLI">
        <CliValue connector={connector} />
      </Row>
      {showProbe && (
        <Row label="Probe">
          <ProbeValue probe={connector.cli.probe} />
        </Row>
      )}
      <Row label="Auth">
        <AuthValue connector={connector} />
      </Row>
      <Row label="Last real run">
        <LastRunValue run={connector.lastRun} />
      </Row>
      <Row label="Last test">
        <LastTestValue check={connector.lastCheck} />
      </Row>
    </dl>
  );
}
