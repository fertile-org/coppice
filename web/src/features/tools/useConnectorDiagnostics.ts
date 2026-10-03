import { useEffect } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import {
  connectorCheckSchema,
  connectorStatusListSchema,
  connectorStatusSchema,
  startConnectorTestSchema,
  type ConnectorCheck,
  type ConnectorStatus,
  type StartConnectorTest,
} from '../../lib/schemas/connectorDiagnostics';

export const TOOL_CONNECTORS_QUERY_KEY = ['tool-connectors'] as const;

export function connectorCheckQueryKey(id: string) {
  return ['connector-checks', id] as const;
}

const CHECK_POLL_MS = 2_000;
const PROBE_POLL_MS = 2_000;

export function isActiveCheck(status: ConnectorCheck['status'] | undefined): boolean {
  return status === 'queued' || status === 'running';
}

async function fetchConnectorStatuses(): Promise<ConnectorStatus[]> {
  const res = await apiFetch('/api/tools/connectors');
  return connectorStatusListSchema.parse(await res.json());
}

async function recheckConnector(id: string): Promise<ConnectorStatus> {
  const res = await apiFetch(`/api/tools/connectors/${id}/check`, { method: 'POST' });
  return connectorStatusSchema.parse(await res.json());
}

async function startConnectorTest(id: string, agentId: string): Promise<StartConnectorTest> {
  const res = await apiFetch(`/api/tools/connectors/${id}/test`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ agentId }),
  });
  return startConnectorTestSchema.parse(await res.json());
}

async function fetchConnectorCheck(id: string): Promise<ConnectorCheck> {
  const res = await apiFetch(`/api/tools/connector-checks/${id}`);
  return connectorCheckSchema.parse(await res.json());
}

/** Polls while any startup probe is still pending (`probedAt` null). */
export function connectorStatusesRefetchInterval(
  list: ConnectorStatus[] | undefined,
): number | false {
  return list?.some((item) => item.probedAt == null) ? PROBE_POLL_MS : false;
}

export function useConnectorStatuses() {
  return useQuery({
    queryKey: TOOL_CONNECTORS_QUERY_KEY,
    queryFn: fetchConnectorStatuses,
    refetchInterval: (query) => connectorStatusesRefetchInterval(query.state.data),
  });
}

export function useRecheckConnector(id: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => recheckConnector(id),
    onSuccess: (fresh) => {
      queryClient.setQueryData<ConnectorStatus[]>(TOOL_CONNECTORS_QUERY_KEY, (list) =>
        list?.map((item) => (item.id === fresh.id ? fresh : item)),
      );
    },
  });
}

export function useStartConnectorTest(id: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (agentId: string) => startConnectorTest(id, agentId),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: TOOL_CONNECTORS_QUERY_KEY });
    },
  });
}

/** Polls while the check is queued or running; refreshes statuses once it settles. */
export function useConnectorCheck(checkId: string | null) {
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: connectorCheckQueryKey(checkId ?? ''),
    queryFn: () => fetchConnectorCheck(checkId!),
    enabled: Boolean(checkId),
    refetchInterval: (current) =>
      isActiveCheck(current.state.data?.status) ? CHECK_POLL_MS : false,
  });
  const status = query.data?.status;
  useEffect(() => {
    if (status && !isActiveCheck(status)) {
      void queryClient.invalidateQueries({ queryKey: TOOL_CONNECTORS_QUERY_KEY });
    }
  }, [status, queryClient]);
  return query;
}
