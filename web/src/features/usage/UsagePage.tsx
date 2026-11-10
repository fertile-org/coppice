import { useMemo, useState, type ReactNode } from 'react';
import { Navigate } from 'react-router-dom';
import type { UsageGroupBy, UsageSummaryRow } from '../../lib/schemas/usage';
import { useConnectors } from '../agents/useAgents';
import { useSession } from '../auth/useSession';
import { useBoards } from '../boards/useBoards';
import { formatTokens } from '../runs/tokenUsage';
import { UsageChart } from './UsageChart';
import {
  GROUP_BY_OPTIONS,
  RANGE_OPTIONS,
  chartSeries,
  connectorNamer,
  coverageLine,
  filledDays,
  inputCount,
  rangeDays,
  rowCanReport,
  rowLabel,
  type CountField,
  type UsageRange,
} from './usageSummaryModel';
import { useUsageSummary } from './useUsageSummary';

const SELECT_CLASS =
  'h-9 rounded-md border border-border bg-surface-raised px-2 font-body text-sm text-text-primary';

function viewerTimeZone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC';
  } catch {
    return 'UTC';
  }
}

function Tile({ label, value, title }: { label: string; value: string; title?: string }) {
  return (
    <div className="rounded-xl border border-border bg-surface-raised px-4 py-3 shadow-card" title={title}>
      <p className="font-body text-xs text-text-secondary">{label}</p>
      <p className="mt-1 font-display text-xl font-semibold text-bark-900">{value}</p>
    </div>
  );
}

function Count({ value, canReport }: { value: number | null; canReport: boolean }) {
  if (value != null) {
    return <span title={value.toLocaleString()}>{formatTokens(value)}</span>;
  }
  return canReport ? (
    <span className="text-text-muted">not reported</span>
  ) : (
    <span className="text-text-muted" title="This connector never reports this count">
      —
    </span>
  );
}

function Th({ children, align = 'right' }: { children: ReactNode; align?: 'left' | 'right' }) {
  return (
    <th
      scope="col"
      className={`px-3 py-2 font-body text-xs font-medium text-text-secondary ${
        align === 'left' ? 'text-left' : 'text-right'
      }`}
    >
      {children}
    </th>
  );
}

/** Settings → Usage: where the workspace's tokens go. Admins only. */
export function UsagePage() {
  const { user, loading } = useSession();
  const isAdmin = user?.role === 'admin';
  const [range, setRange] = useState<UsageRange>('30');
  const [groupBy, setGroupBy] = useState<UsageGroupBy>('model');
  const [board, setBoard] = useState<string>('');
  const [connector, setConnector] = useState<string>('');
  const tz = useMemo(() => viewerTimeZone(), []);
  const { from, to } = useMemo(() => rangeDays(range), [range]);
  const summary = useUsageSummary(
    { from, to, tz, groupBy, board: board || null, connector: connector || null },
    isAdmin,
  );
  const connectors = useConnectors({ enabled: isAdmin });
  const boards = useBoards();

  if (loading) {
    return <p className="font-body text-sm text-text-muted">Loading session…</p>;
  }
  if (!isAdmin) {
    return <Navigate to="/boards" replace />;
  }

  const connectorName = connectorNamer(connectors.data);
  const coverageOf = (id: string) => connectors.data?.find((c) => c.id === id)?.usageCoverage;
  const data = summary.data;
  const labelOf = (row: Pick<UsageSummaryRow, 'key' | 'label'>) =>
    rowLabel(groupBy, row, connectorName);
  const rowByKey = new Map((data?.rows ?? []).map((row) => [row.key, row]));
  const { keys, hasOther } = chartSeries(data?.rows ?? []);
  const total = data?.totals.total ?? 0;
  const canReport = (row: UsageSummaryRow, field: CountField) =>
    rowCanReport(groupBy, row, field, coverageOf);

  return (
    <div className="space-y-6">
      <div>
        <h1 className="font-display text-2xl font-semibold text-bark-900">Usage</h1>
        <p className="mt-2 max-w-2xl font-body text-text-secondary">
          Tokens your agents spent, by model, agent, board, connector or job type. Counts
          come from what each connector reports; Coppice doesn't estimate missing ones.
        </p>
      </div>

      <div className="flex flex-wrap items-end gap-3">
        <label className="flex flex-col gap-1 font-body text-xs text-text-secondary">
          Range
          <select
            className={SELECT_CLASS}
            value={range}
            onChange={(event) => setRange(event.target.value as UsageRange)}
          >
            {RANGE_OPTIONS.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </label>
        <label className="flex flex-col gap-1 font-body text-xs text-text-secondary">
          Board
          <select className={SELECT_CLASS} value={board} onChange={(event) => setBoard(event.target.value)}>
            <option value="">All boards and Workspace</option>
            {(boards.data ?? []).map((b) => (
              <option key={b.id} value={b.id}>
                {b.name}
              </option>
            ))}
          </select>
        </label>
        <label className="flex flex-col gap-1 font-body text-xs text-text-secondary">
          Connector
          <select
            className={SELECT_CLASS}
            value={connector}
            onChange={(event) => setConnector(event.target.value)}
          >
            <option value="">All connectors</option>
            {(connectors.data ?? [])
              .filter((c) => c.id !== 'mock' || c.enabled)
              .map((c) => (
                <option key={c.id} value={c.id}>
                  {c.displayName}
                </option>
              ))}
          </select>
        </label>
        <label className="flex flex-col gap-1 font-body text-xs text-text-secondary">
          Group by
          <select
            className={SELECT_CLASS}
            value={groupBy}
            onChange={(event) => setGroupBy(event.target.value as UsageGroupBy)}
          >
            {GROUP_BY_OPTIONS.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </label>
      </div>

      {summary.isError ? (
        <p className="rounded-md bg-danger-muted px-3 py-2 font-body text-sm text-danger">
          Couldn't load usage. Reload the page to try again.
        </p>
      ) : null}

      {data ? (
        <>
          <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
            <Tile label="Total tokens" value={formatTokens(total)} title={total.toLocaleString()} />
            <Tile label="Tracked runs" value={data.coverage.trackedRuns.toLocaleString()} />
            <Tile
              label="Tokens per Done ticket"
              value={data.perDoneTicket ? formatTokens(Math.round(data.perDoneTicket.median)) : '—'}
              title={
                data.perDoneTicket
                  ? `Median over ${data.perDoneTicket.tickets} top-level tickets, subtickets included. Average ${formatTokens(Math.round(data.perDoneTicket.average))}.`
                  : 'No top-level ticket with tracked usage reached Done in this range.'
              }
            />
            <Tile label="Active days" value={data.totals.activeDays.toLocaleString()} />
          </div>

          <p className="font-body text-sm text-text-secondary" data-testid="usage-coverage">
            {coverageLine(data.coverage, connectorName)}
          </p>

          <section className="rounded-xl border border-border bg-surface-raised p-5 shadow-card">
            <h2 className="mb-3 font-display text-base font-semibold text-bark-900">Tokens per day</h2>
            <UsageChart
              days={filledDays(data.daily, from, to)}
              keys={keys}
              hasOther={hasOther}
              labelOf={(key) => labelOf(rowByKey.get(key) ?? { key, label: null })}
            />
          </section>

          <section className="overflow-x-auto rounded-xl border border-border bg-surface-raised shadow-card">
            <table className="w-full min-w-[640px] border-collapse">
              <thead className="border-b border-border">
                <tr>
                  <Th align="left">{GROUP_BY_OPTIONS.find((o) => o.value === groupBy)?.label}</Th>
                  <Th>In</Th>
                  <Th>Cached</Th>
                  <Th>Out</Th>
                  <Th>No breakdown</Th>
                  <Th>Total</Th>
                  <Th>%</Th>
                  <Th>Runs</Th>
                </tr>
              </thead>
              <tbody>
                {data.rows.length === 0 ? (
                  <tr>
                    <td colSpan={8} className="px-3 py-4 font-body text-sm text-text-muted">
                      No tokens in this range.
                    </td>
                  </tr>
                ) : (
                  data.rows.map((row) => (
                    <tr key={row.key} className="border-b border-border last:border-b-0">
                      <td className="px-3 py-2 font-body text-sm text-bark-900">
                        {labelOf(row)}
                        {groupBy === 'model' && row.connector ? (
                          <span className="ml-2 font-body text-xs text-text-muted">
                            {connectorName(row.connector)}
                          </span>
                        ) : null}
                      </td>
                      <td className="px-3 py-2 text-right font-body text-sm tabular-nums">
                        <Count value={inputCount(row.tokens)} canReport={canReport(row, 'input')} />
                      </td>
                      <td className="px-3 py-2 text-right font-body text-sm tabular-nums">
                        <Count value={row.tokens.cacheRead} canReport={canReport(row, 'cacheRead')} />
                      </td>
                      <td className="px-3 py-2 text-right font-body text-sm tabular-nums">
                        <Count value={row.tokens.output} canReport={canReport(row, 'output')} />
                      </td>
                      <td className="px-3 py-2 text-right font-body text-sm tabular-nums">
                        {row.unsplit > 0 ? formatTokens(row.unsplit) : '0'}
                      </td>
                      <td
                        className="px-3 py-2 text-right font-body text-sm font-semibold tabular-nums"
                        title={row.total.toLocaleString()}
                      >
                        {formatTokens(row.total)}
                      </td>
                      <td className="px-3 py-2 text-right font-body text-sm tabular-nums text-text-secondary">
                        {total > 0 ? `${Math.round((row.total / total) * 100)}%` : '—'}
                      </td>
                      <td className="px-3 py-2 text-right font-body text-sm tabular-nums text-text-secondary">
                        {row.runs}
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </section>
        </>
      ) : summary.isLoading ? (
        <p className="font-body text-sm text-text-muted">Loading usage…</p>
      ) : null}
    </div>
  );
}
