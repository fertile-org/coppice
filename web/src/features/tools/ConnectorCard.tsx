import { useState } from 'react';
import { Button } from '../../components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogTitle,
} from '../../components/ui/dialog';
import { parseApiErrorMessage } from '../../lib/api';
import type { ConnectorStatus } from '../../lib/schemas/connectorDiagnostics';
import { useAgents } from '../agents/useAgents';
import { ConnectorStatusList } from './ConnectorStatusList';
import { ConnectorTestSection } from './ConnectorTestSection';
import { turnOffConfirm } from './connectorCopy';
import { useRecheckConnector, useSetConnectorEnabled } from './useConnectorDiagnostics';

function enabledPillClass(enabled: boolean): string {
  const base =
    'inline-flex shrink-0 items-center rounded-full border px-2 py-0.5 font-body text-xs';
  return enabled
    ? `${base} border-success-muted bg-success-muted text-success`
    : `${base} border-border bg-paper-200 text-text-secondary`;
}

function needsHelp(connector: ConnectorStatus): boolean {
  return (
    Boolean(connector.probedAt) &&
    (!connector.cli.found || connector.auth.status === 'not_found')
  );
}

export function ConnectorCard({ connector }: { connector: ConnectorStatus }) {
  const recheck = useRecheckConnector(connector.id);
  const setEnabled = useSetConnectorEnabled();
  const agents = useAgents();
  const [error, setError] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const users = (agents.data ?? []).filter((agent) => agent.connector === connector.id).length;
  const countKnown = agents.isSuccess;

  async function handleRecheck() {
    setError(null);
    try {
      await recheck.mutateAsync();
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Check failed.'));
    }
  }

  async function applyEnabled(enabled: boolean) {
    setError(null);
    setConfirming(false);
    try {
      await setEnabled.mutateAsync({ id: connector.id, enabled });
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Could not update this connector.'));
    }
  }

  function handleToggle() {
    if (connector.enabled) {
      if (!countKnown) return;
      if (users > 0) {
        setConfirming(true);
        return;
      }
      void applyEnabled(false);
      return;
    }
    void applyEnabled(true);
  }

  return (
    <div
      data-testid={`connector-card-${connector.id}`}
      className="rounded-xl border border-border bg-surface-raised p-4 shadow-card"
    >
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <h3 className="font-display text-base font-semibold text-text-primary">
            {connector.displayName}
          </h3>
          <span className="font-mono text-xs text-text-muted">{connector.id}</span>
          <button
            type="button"
            role="switch"
            aria-checked={connector.enabled}
            aria-label={`Enabled ${connector.displayName}`}
            disabled={setEnabled.isPending || (connector.enabled && !countKnown)}
            onClick={handleToggle}
            className={enabledPillClass(connector.enabled)}
          >
            {connector.enabled ? 'Enabled' : 'Disabled'}
          </button>
        </div>
        <Button
          type="button"
          variant="secondary"
          size="sm"
          loading={recheck.isPending}
          onClick={() => void handleRecheck()}
        >
          Run check
        </Button>
      </div>

      <ConnectorStatusList connector={connector} />

      <Dialog open={confirming} onOpenChange={(open) => !open && setConfirming(false)}>
        <DialogContent>
          <DialogTitle className="sr-only">Turn off {connector.displayName}</DialogTitle>
          <p className="font-body text-sm text-text-primary">
            {turnOffConfirm(connector.displayName, users)}
          </p>
          <div className="mt-4 flex justify-end gap-2">
            <Button type="button" variant="secondary" onClick={() => setConfirming(false)}>
              Cancel
            </Button>
            <Button type="button" variant="destructive" onClick={() => void applyEnabled(false)}>
              Turn off
            </Button>
          </div>
        </DialogContent>
      </Dialog>

      {needsHelp(connector) && (
        <div className="mt-3 rounded-md border border-warning-muted bg-warning-muted/30 px-3 py-2 font-body text-xs text-text-secondary">
          {connector.authHint && <p>{connector.authHint}</p>}
          {connector.docsUrl && (
            <a
              href={connector.docsUrl}
              target="_blank"
              rel="noreferrer"
              className="font-medium text-moss-700 underline-offset-2 hover:underline"
            >
              Install docs
            </a>
          )}
        </div>
      )}

      {error && (
        <p role="alert" className="mt-2 font-body text-xs text-danger">
          {error}
        </p>
      )}

      <ConnectorTestSection connector={connector} />
    </div>
  );
}
