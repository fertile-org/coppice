import { Search } from 'lucide-react';
import { useMemo, useState } from 'react';
import { Input } from '../../components/ui/input';
import type { ConnectorStatus } from '../../lib/schemas/connectorDiagnostics';
import { ConnectorCard } from './ConnectorCard';
import { useConnectorStatuses } from './useConnectorDiagnostics';

function matchesSearch(connector: ConnectorStatus, terms: string[]): boolean {
  const haystack = `${connector.displayName} ${connector.id}`.toLowerCase();
  return terms.every((term) => haystack.includes(term));
}

export function ConnectorsTab() {
  const { data: connectors, isLoading, isError } = useConnectorStatuses();
  const [search, setSearch] = useState('');
  const terms = useMemo(
    () => search.trim().toLowerCase().split(/\s+/).filter(Boolean),
    [search],
  );
  const visible = useMemo(
    () => connectors?.filter((connector) => matchesSearch(connector, terms)),
    [connectors, terms],
  );

  return (
    <div className="space-y-4">
      <p className="max-w-2xl font-body text-sm text-text-secondary">
        Each connector runs a CLI installed and logged in on the server machine.
        Run check re-probes the CLI locally; Test connection runs a short agent
        run through the Coppice gateway.
      </p>
      {connectors && connectors.length > 0 && (
        <div className="relative max-w-md">
          <Search
            className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-text-muted"
            aria-hidden="true"
          />
          <Input
            type="search"
            aria-label="Search connectors"
            placeholder="Search connectors…"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            className="h-10 pl-9"
          />
        </div>
      )}
      {isLoading && (
        <p className="font-body text-sm text-text-muted">Loading connectors…</p>
      )}
      {isError && (
        <p className="font-body text-sm text-danger">Unable to load connectors.</p>
      )}
      {connectors && connectors.length === 0 && (
        <p className="font-body text-sm text-text-muted">No connectors to diagnose.</p>
      )}
      {connectors && connectors.length > 0 && visible?.length === 0 && (
        <p className="font-body text-sm text-text-muted">No connector matches “{search.trim()}”.</p>
      )}
      <div className="grid gap-4 lg:grid-cols-2">
        {visible?.map((connector) => (
          <ConnectorCard key={connector.id} connector={connector} />
        ))}
      </div>
    </div>
  );
}
