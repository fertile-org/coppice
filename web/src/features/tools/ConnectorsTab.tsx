import { ConnectorCard } from './ConnectorCard';
import { useConnectorStatuses } from './useConnectorDiagnostics';

export function ConnectorsTab() {
  const { data: connectors, isLoading, isError } = useConnectorStatuses();

  return (
    <div className="space-y-4">
      <p className="max-w-2xl font-body text-sm text-text-secondary">
        Each connector runs a CLI installed and logged in on the server machine.
        Run check re-probes the CLI locally; Test connection runs a short agent
        run through the Coppice gateway.
      </p>
      {isLoading && (
        <p className="font-body text-sm text-text-muted">Loading connectors…</p>
      )}
      {isError && (
        <p className="font-body text-sm text-danger">Unable to load connectors.</p>
      )}
      {connectors && connectors.length === 0 && (
        <p className="font-body text-sm text-text-muted">No connectors to diagnose.</p>
      )}
      <div className="grid gap-4 lg:grid-cols-2">
        {connectors?.map((connector) => (
          <ConnectorCard key={connector.id} connector={connector} />
        ))}
      </div>
    </div>
  );
}
