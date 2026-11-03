import { useState } from 'react';
import { Button } from '../../components/ui/button';
import { parseApiErrorMessage } from '../../lib/api';
import type {
  ConnectorCheck,
  ConnectorStatus,
} from '../../lib/schemas/connectorDiagnostics';
import { ConnectorStatusList } from './ConnectorStatusList';
import { ConnectorTestSection } from './ConnectorTestSection';
import { UncoveredModels } from './UncoveredModels';
import { useRecheckConnector } from './useConnectorDiagnostics';

function needsHelp(connector: ConnectorStatus): boolean {
  return (
    Boolean(connector.probedAt) &&
    (!connector.cli.found || connector.auth.status === 'not_found')
  );
}

interface ConnectorDetailsProps {
  connector: ConnectorStatus;
  checkId: string | null;
  check: ConnectorCheck | undefined;
  checkError: boolean;
  onStarted: (checkId: string) => void;
}

export function ConnectorDetails({
  connector,
  checkId,
  check,
  checkError,
  onStarted,
}: ConnectorDetailsProps) {
  const recheck = useRecheckConnector(connector.id);
  const [error, setError] = useState<string | null>(null);

  async function handleRecheck() {
    setError(null);
    try {
      await recheck.mutateAsync();
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Check failed.'));
    }
  }

  return (
    <div>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <span className="font-mono text-xs text-text-muted">{connector.id}</span>
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

      <ConnectorTestSection
        connector={connector}
        checkId={checkId}
        check={check}
        checkError={checkError}
        onStarted={onStarted}
      />

      <UncoveredModels connector={connector} />
    </div>
  );
}
