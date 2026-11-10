import type { Connector, UsageCoverage } from '../../lib/schemas/connector';
import type { TokenCounts, UsageGroupBy, UsageSummary, UsageSummaryRow } from '../../lib/schemas/usage';

export const GROUP_BY_OPTIONS: { value: UsageGroupBy; label: string }[] = [
  { value: 'model', label: 'Model' },
  { value: 'agent', label: 'Agent' },
  { value: 'board', label: 'Board' },
  { value: 'connector', label: 'Connector' },
  { value: 'jobType', label: 'Job type' },
];

export type UsageRange = '7' | '30' | '90' | 'all';

export const RANGE_OPTIONS: { value: UsageRange; label: string }[] = [
  { value: '7', label: 'Last 7 days' },
  { value: '30', label: 'Last 30 days' },
  { value: '90', label: 'Last 90 days' },
  { value: 'all', label: 'All time' },
];

function isoDay(date: Date): string {
  const y = date.getFullYear();
  const m = String(date.getMonth() + 1).padStart(2, '0');
  const d = String(date.getDate()).padStart(2, '0');
  return `${y}-${m}-${d}`;
}

/** The range's first and last day in the viewer's local calendar, today included. */
export function rangeDays(range: UsageRange, today: Date = new Date()): { from: string | null; to: string | null } {
  if (range === 'all') return { from: null, to: null };
  const from = new Date(today);
  from.setDate(from.getDate() - (Number(range) - 1));
  return { from: isoDay(from), to: isoDay(today) };
}

const JOB_TYPE_LABELS: Record<string, string> = {
  work_on_ticket: 'Ticket work',
  plan_ticket: 'Planning',
  respond_to_mention: 'Mention replies',
  verify_parent: 'Parent checks',
  chat_turn: 'Chat',
  workspace_overhead: 'Workspace overhead',
  unknown: 'Unknown job type',
};

function sentence(snake: string): string {
  const words = snake.replace(/_/g, ' ').trim();
  return words ? words[0].toUpperCase() + words.slice(1) : snake;
}

/** What a row is called on the page. */
export function rowLabel(
  groupBy: UsageGroupBy,
  row: Pick<UsageSummaryRow, 'key' | 'label'>,
  connectorName: (id: string) => string,
): string {
  switch (groupBy) {
    case 'model':
      return row.label ?? 'Model not reported';
    case 'agent':
      return row.label ?? 'Deleted agent';
    case 'board':
      if (row.key === 'workspace') return 'Workspace';
      return row.label ?? 'Deleted board';
    case 'connector':
      if (row.key === 'unknown') return 'Unknown connector';
      return row.label ?? connectorName(row.key);
    case 'jobType':
      return JOB_TYPE_LABELS[row.key] ?? sentence(row.key);
  }
}

/** The page's own name for a connector id, falling back to the id. */
export function connectorNamer(connectors: Connector[] | undefined): (id: string) => string {
  return (id) => connectors?.find((c) => c.id === id)?.displayName ?? id;
}

export type CountField = 'input' | 'cacheRead' | 'output';

/**
 * Whether a count column can ever be filled for a row: false only when the
 * row's connector is known and its coverage leaves the count out. Rows that
 * mix connectors (agent, board, job type) never get "—".
 */
export function rowCanReport(
  groupBy: UsageGroupBy,
  row: Pick<UsageSummaryRow, 'key' | 'connector'>,
  field: CountField,
  coverageOf: (connector: string) => UsageCoverage | null | undefined,
): boolean {
  const connector = groupBy === 'model' || groupBy === 'connector' ? (row.connector ?? null) : null;
  if (!connector) return true;
  const coverage = coverageOf(connector);
  if (!coverage) return true;
  if (field === 'input') return coverage.fields.input || coverage.fields.cacheWrite;
  return coverage.fields[field];
}

/** "In" on the page: uncached input plus cache writes, as on the Runs tab. */
export function inputCount(tokens: TokenCounts): number | null {
  if (tokens.input == null && tokens.cacheWrite == null) return null;
  return (tokens.input ?? 0) + (tokens.cacheWrite ?? 0);
}

/** "Counts 42 of 60 runs in this range. Not tracked: Cursor (12 runs)." */
export function coverageLine(
  coverage: UsageSummary['coverage'],
  connectorName: (id: string) => string,
): string {
  const all = coverage.trackedRuns + coverage.untrackedRuns;
  const runs = (n: number) => `${n} ${n === 1 ? 'run' : 'runs'}`;
  if (all === 0) return 'No runs in this range.';
  const counts = `Counts ${coverage.trackedRuns} of ${runs(all)} in this range.`;
  if (coverage.untrackedByConnector.length === 0) return counts;
  const untracked = coverage.untrackedByConnector
    .map((c) => `${connectorName(c.connector)} (${runs(c.runs)})`)
    .join(', ');
  return `${counts} Not tracked: ${untracked}.`;
}

/** Most series a chart colors; the rest fold into one "Other" series. */
export const MAX_SERIES = 7;

export const OTHER_KEY = '__other__';

/**
 * The chart's series: the largest groups, in table order, then "Other".
 * Colors follow the order the API returns (largest first) for this view.
 */
export function chartSeries(rows: UsageSummaryRow[]): { keys: string[]; hasOther: boolean } {
  const keys = rows.slice(0, MAX_SERIES).map((r) => r.key);
  return { keys, hasOther: rows.length > MAX_SERIES };
}

/** One day's stack, with groups beyond the series folded into "Other". */
export function dayStack(
  byKey: Record<string, number>,
  keys: string[],
  hasOther: boolean,
): { key: string; value: number }[] {
  const stack = keys.map((key) => ({ key, value: byKey[key] ?? 0 }));
  if (hasOther) {
    const shown = new Set(keys);
    const other = Object.entries(byKey)
      .filter(([key]) => !shown.has(key))
      .reduce((sum, [, value]) => sum + value, 0);
    stack.push({ key: OTHER_KEY, value: other });
  }
  return stack;
}

/** Every day from the first to the last day with tokens, so gaps show as gaps. */
export function filledDays(
  daily: UsageSummary['daily'],
  from: string | null,
  to: string | null,
): UsageSummary['daily'] {
  const start = from ?? daily[0]?.day;
  const end = to ?? daily[daily.length - 1]?.day;
  if (!start || !end) return [];
  const byDay = new Map(daily.map((d) => [d.day, d.byKey]));
  const days: UsageSummary['daily'] = [];
  const cursor = new Date(`${start}T00:00:00`);
  const last = new Date(`${end}T00:00:00`);
  // Cap at a year of columns; "All time" past that keeps only days with tokens.
  if ((last.getTime() - cursor.getTime()) / 86_400_000 > 366) return daily;
  while (cursor <= last) {
    const day = isoDay(cursor);
    days.push({ day, byKey: byDay.get(day) ?? {} });
    cursor.setDate(cursor.getDate() + 1);
  }
  return days;
}
