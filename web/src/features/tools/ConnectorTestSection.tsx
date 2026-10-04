import { useId, useState } from 'react';
import { Link } from 'react-router-dom';
import { Button } from '../../components/ui/button';
import { Combobox } from '../../components/ui/combobox';
import { ApiError, parseApiErrorMessage } from '../../lib/api';
import type {
  ConnectorCheck,
  ConnectorStatus,
} from '../../lib/schemas/connectorDiagnostics';
import { useAgents } from '../agents/useAgents';
import { CHECK_STATUS_LABELS, checkStatusClass } from './connectorFormat';
import {
  isActiveCheck,
  useConnectorCheck,
  useStartConnectorTest,
} from './useConnectorDiagnostics';

function startErrorFallback(err: unknown): string {
  if (!(err instanceof ApiError)) return 'Unable to start test.';
  switch (err.status) {
    case 400:
      return 'This agent cannot test this connector (it uses another connector, or the connector is disabled).';
    case 404:
      return 'Connector or agent not found.';
    case 409:
      return 'A test is already running for this connector.';
    default:
      return 'Unable to start test.';
  }
}

function CheckResult({
  connectorId,
  check,
  isError,
}: {
  connectorId: string;
  check: ConnectorCheck | undefined;
  isError: boolean;
}) {
  return (
    <div
      data-testid={`connector-test-result-${connectorId}`}
      className="mt-2 font-body text-xs"
      aria-live="polite"
    >
      {isError && <p className="text-danger">Unable to load test status.</p>}
      {!isError && !check && <p className="text-info">Starting…</p>}
      {check && (
        <>
          <p className={checkStatusClass(check.status)}>{CHECK_STATUS_LABELS[check.status]}</p>
          {check.failure && <p className="text-danger">{check.failure}</p>}
        </>
      )}
    </div>
  );
}

export function ConnectorTestSection({ connector }: { connector: ConnectorStatus }) {
  const { data: agents, isSuccess: agentsLoaded } = useAgents();
  const startTest = useStartConnectorTest(connector.id);
  const selectId = useId();
  const [open, setOpen] = useState(false);
  const [agentId, setAgentId] = useState('');
  const [checkId, setCheckId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const matching = agents?.filter((agent) => agent.connector === connector.id) ?? [];
  const selected = agentId || (matching.length === 1 ? matching[0].id : '');
  const lastCheck = connector.lastCheck;
  const activeId =
    checkId ?? (lastCheck && isActiveCheck(lastCheck.status) ? lastCheck.id : null);
  const checkQuery = useConnectorCheck(activeId);
  const testing =
    Boolean(activeId) &&
    !checkQuery.isError &&
    (!checkQuery.data || isActiveCheck(checkQuery.data.status));
  const noAgents = agentsLoaded && matching.length === 0;
  const canTest = connector.enabled && matching.length > 0 && !testing;

  async function handleStart() {
    if (!selected) return;
    setError(null);
    try {
      const started = await startTest.mutateAsync(selected);
      setCheckId(started.checkId);
      setOpen(false);
    } catch (err) {
      setError(parseApiErrorMessage(err, startErrorFallback(err)));
    }
  }

  return (
    <div className="mt-3 border-t border-border pt-3">
      <Button
        type="button"
        variant="secondary"
        size="sm"
        disabled={!canTest}
        title={connector.enabled ? undefined : 'Enable this connector first'}
        aria-expanded={open}
        onClick={() => setOpen((prev) => !prev)}
      >
        Test connection
      </Button>
      {connector.enabled && noAgents && (
        <p className="mt-2 font-body text-xs text-text-muted">
          Create an agent with this connector first.{' '}
          <Link to="/agents" className="font-medium text-moss-700 underline-offset-2 hover:underline">
            Agents
          </Link>
        </p>
      )}
      {open && canTest && (
        <div className="mt-2 flex flex-wrap items-end gap-2">
          <div>
            <label htmlFor={selectId} className="mb-1 block font-body text-xs text-text-secondary">
              Agent
            </label>
            <Combobox
              id={selectId}
              value={selected}
              onValueChange={setAgentId}
              placeholder="Choose an agent"
              searchPlaceholder="Search agents…"
              className="min-w-[14rem]"
              triggerClassName="h-9"
              options={matching.map((agent) => ({
                value: agent.id,
                label: agent.model ? `${agent.name} · ${agent.model}` : agent.name,
              }))}
            />
          </div>
          <Button
            type="button"
            size="sm"
            disabled={!selected}
            loading={startTest.isPending}
            onClick={() => void handleStart()}
          >
            Start test
          </Button>
        </div>
      )}
      {activeId && (
        <CheckResult
          connectorId={connector.id}
          check={checkQuery.data}
          isError={checkQuery.isError}
        />
      )}
      {error && (
        <p role="alert" className="mt-2 font-body text-xs text-danger">
          {error}
        </p>
      )}
    </div>
  );
}
