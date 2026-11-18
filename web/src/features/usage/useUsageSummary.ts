import { keepPreviousData, useQuery } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import {
  usageSummarySchema,
  type UsageGroupBy,
  type UsageSummary,
} from '../../lib/schemas/usage';

export interface UsageSummaryParams {
  /** `YYYY-MM-DD` in `tz`, inclusive; `null` is open. */
  from: string | null;
  to: string | null;
  tz: string;
  groupBy: UsageGroupBy;
  board: string | null;
  connector: string | null;
}

export function usageSummaryPath(params: UsageSummaryParams): string {
  const query = new URLSearchParams({ groupBy: params.groupBy, tz: params.tz });
  if (params.from) query.set('from', params.from);
  if (params.to) query.set('to', params.to);
  if (params.board) query.set('board', params.board);
  if (params.connector) query.set('connector', params.connector);
  return `/api/usage/summary?${query.toString()}`;
}

async function fetchUsageSummary(params: UsageSummaryParams): Promise<UsageSummary> {
  const response = await apiFetch(usageSummaryPath(params));
  return usageSummarySchema.parse(await response.json());
}

export function useUsageSummary(params: UsageSummaryParams, enabled: boolean) {
  return useQuery({
    queryKey: ['usage-summary', params] as const,
    queryFn: () => fetchUsageSummary(params),
    enabled,
    // Keep the last result on screen while a new range or grouping loads.
    placeholderData: keepPreviousData,
  });
}
