import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { RepoDrawer } from '../repos/RepoDrawer';
import { useAgents } from '../agents/useAgents';
import { cn } from '../../lib/utils';
import type {
  ConnectorCheck,
  ConnectorStatus,
} from '../../lib/schemas/connectorDiagnostics';
import { ConnectorDetails } from './ConnectorDetails';
import { ConnectorRow } from './ConnectorRow';
import {
  activeCheckId,
  type ConnectorFilter,
  visibleConnectors,
} from './connectorList';
import {
  useConnectorCheck,
  useConnectorStatuses,
} from './useConnectorDiagnostics';

const FILTERS: { id: ConnectorFilter; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'enabled', label: 'Enabled' },
  { id: 'disabled', label: 'Disabled' },
  { id: 'attention', label: 'Needs attention' },
];

type CheckState = { check: ConnectorCheck | undefined; isError: boolean };

function ConnectorCheckPoller({
  connectorId,
  checkId,
  onChange,
}: {
  connectorId: string;
  checkId: string;
  onChange: (connectorId: string, state: CheckState) => void;
}) {
  const { data, isError } = useConnectorCheck(checkId);
  useEffect(() => {
    onChange(connectorId, { check: data, isError });
  }, [connectorId, data, isError, onChange]);
  return null;
}

export function ConnectorsTab() {
  const { data: connectors, isLoading, isError } = useConnectorStatuses();
  const agents = useAgents();
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<ConnectorFilter>('all');
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [sessionCheckIds, setSessionCheckIds] = useState<Record<string, string>>({});
  const [checks, setChecks] = useState<Record<string, CheckState>>({});
  const searchRef = useRef<HTMLInputElement>(null);
  const rowButtons = useRef(new Map<string, HTMLButtonElement>());
  const focusIdRef = useRef<string | null>(null);

  const list = connectors ?? [];
  const { visible, counts } = useMemo(
    () => visibleConnectors(list, query, filter),
    [list, query, filter],
  );
  const selected = list.find((connector) => connector.id === selectedId) ?? null;
  const [shown, setShown] = useState(selected);
  if (selected && selected !== shown) setShown(selected);

  const agentCounts = useMemo(() => {
    if (!agents.isSuccess) return null;
    const map = new Map<string, number>();
    for (const agent of agents.data) {
      map.set(agent.connector, (map.get(agent.connector) ?? 0) + 1);
    }
    return map;
  }, [agents.data, agents.isSuccess]);

  const pollTargets = useMemo(() => {
    const targets: { connectorId: string; checkId: string }[] = [];
    for (const connector of list) {
      const checkId = activeCheckId(connector, sessionCheckIds[connector.id]);
      if (checkId) targets.push({ connectorId: connector.id, checkId });
    }
    return targets;
  }, [list, sessionCheckIds]);

  const handleCheckChange = useCallback(
    (connectorId: string, state: CheckState) => {
      setChecks((prev) => {
        const existing = prev[connectorId];
        if (
          existing &&
          existing.check === state.check &&
          existing.isError === state.isError
        ) {
          return prev;
        }
        return { ...prev, [connectorId]: state };
      });
    },
    [],
  );

  function open(id: string) {
    focusIdRef.current = id;
    setSelectedId(id);
  }

  function close() {
    setSelectedId(null);
  }

  function restoreFocus() {
    const id = focusIdRef.current;
    const button = id ? rowButtons.current.get(id) : undefined;
    if (button) button.focus();
    else searchRef.current?.focus();
  }

  function onStarted(connectorId: string, checkId: string) {
    setSessionCheckIds((prev) => ({ ...prev, [connectorId]: checkId }));
  }

  function checkStateFor(connector: ConnectorStatus): CheckState {
    return checks[connector.id] ?? { check: undefined, isError: false };
  }

  function hasLiveCheck(connector: ConnectorStatus): boolean {
    return Boolean(activeCheckId(connector, sessionCheckIds[connector.id]));
  }

  return (
    <div className="space-y-4">
      {pollTargets.map(({ connectorId, checkId }) => (
        <ConnectorCheckPoller
          key={`${connectorId}:${checkId}`}
          connectorId={connectorId}
          checkId={checkId}
          onChange={handleCheckChange}
        />
      ))}
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
      {connectors && connectors.length > 0 && (
        <>
          <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
            <input
              ref={searchRef}
              type="search"
              aria-label="Search connectors"
              placeholder="Search connectors"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              className="field-control min-w-0 flex-1 px-3 py-2 font-body text-sm"
            />
            <div className="flex flex-wrap gap-1.5">
              {FILTERS.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  aria-pressed={filter === item.id}
                  onClick={() => setFilter(item.id)}
                  className={cn(
                    'rounded-full border px-2.5 py-1 font-body text-xs',
                    filter === item.id
                      ? 'border-moss-600 bg-moss-500/15 text-text-primary'
                      : 'border-border text-text-secondary hover:text-text-primary',
                    item.id === 'attention' && 'text-warning',
                  )}
                >
                  {item.label} {counts[item.id]}
                </button>
              ))}
            </div>
          </div>
          {visible.length === 0 ? (
            <p className="rounded-xl border border-border bg-surface-raised px-4 py-8 text-center font-body text-sm text-text-muted shadow-card">
              No connectors match.
            </p>
          ) : (
            <ul className="overflow-hidden rounded-xl border border-border bg-surface-raised">
              {visible.map((connector) => {
                const state = checkStateFor(connector);
                return (
                  <ConnectorRow
                    key={connector.id}
                    connector={connector}
                    selected={connector.id === selectedId}
                    agentCount={
                      agentCounts ? (agentCounts.get(connector.id) ?? 0) : null
                    }
                    liveCheck={state.check}
                    checkError={state.isError}
                    hasLiveCheck={hasLiveCheck(connector)}
                    onOpen={() => open(connector.id)}
                    onButtonRef={(el) => {
                      if (el) rowButtons.current.set(connector.id, el);
                      else rowButtons.current.delete(connector.id);
                    }}
                  />
                );
              })}
            </ul>
          )}
          <RepoDrawer
            open={selected !== null}
            ariaLabel="Connector details"
            title={shown?.displayName ?? ''}
            onClose={close}
            onRestoreFocus={restoreFocus}
          >
            {shown && (
              <ConnectorDetails
                key={shown.id}
                connector={shown}
                checkId={activeCheckId(shown, sessionCheckIds[shown.id])}
                check={checkStateFor(shown).check}
                checkError={checkStateFor(shown).isError}
                onStarted={(checkId) => onStarted(shown.id, checkId)}
              />
            )}
          </RepoDrawer>
        </>
      )}
    </div>
  );
}
