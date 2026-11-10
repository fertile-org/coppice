import { useState } from 'react';
import type { UsageSummary } from '../../lib/schemas/usage';
import { formatTokens } from '../runs/tokenUsage';
import { OTHER_KEY, dayStack } from './usageSummaryModel';

const SERIES_COLORS = [1, 2, 3, 4, 5, 6, 7].map((n) => `var(--chart-series-${n})`);
const OTHER_COLOR = 'var(--chart-series-other)';

/** The color of a series, by its place in the order (never by its value). */
function colorOf(key: string, keys: string[]): string {
  if (key === OTHER_KEY) return OTHER_COLOR;
  return SERIES_COLORS[keys.indexOf(key) % SERIES_COLORS.length];
}

/** A clean axis maximum: 1, 2 or 5 times a power of ten. */
function niceMax(value: number): number {
  if (value <= 0) return 1;
  const power = 10 ** Math.floor(Math.log10(value));
  const step = [1, 2, 5, 10].find((m) => m * power >= value) ?? 10;
  return step * power;
}

function shortDay(day: string): string {
  const date = new Date(`${day}T00:00:00`);
  return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

interface UsageChartProps {
  days: UsageSummary['daily'];
  /** Series keys in color order; groups beyond them fold into "Other". */
  keys: string[];
  hasOther: boolean;
  labelOf: (key: string) => string;
}

/** Tokens per day, stacked by the selected group. */
export function UsageChart({ days, keys, hasOther, labelOf }: UsageChartProps) {
  const [active, setActive] = useState<number | null>(null);
  const stacks = days.map((d) => dayStack(d.byKey, keys, hasOther));
  const totals = stacks.map((stack) => stack.reduce((sum, s) => sum + s.value, 0));
  const max = niceMax(Math.max(0, ...totals));
  const ticks = [max, max / 2, 0];
  const legend = [...keys, ...(hasOther ? [OTHER_KEY] : [])];
  const nameOf = (key: string) => (key === OTHER_KEY ? 'Other' : labelOf(key));
  // Label a few days only: the first, the last, and evenly between.
  const labelEvery = Math.max(1, Math.ceil(days.length / 6));

  if (days.length === 0) {
    return (
      <p className="font-body text-sm text-text-muted">No tokens in this range.</p>
    );
  }

  return (
    <figure className="space-y-3" aria-label="Tokens per day">
      {legend.length > 1 ? (
        <ul className="flex flex-wrap gap-x-4 gap-y-1" aria-label="Legend">
          {legend.map((key) => (
            <li key={key} className="flex items-center gap-1.5 font-body text-xs text-text-secondary">
              <span
                aria-hidden
                className="size-2.5 rounded-sm"
                style={{ backgroundColor: colorOf(key, keys) }}
              />
              {nameOf(key)}
            </li>
          ))}
        </ul>
      ) : null}
      <div className="flex gap-2">
        <div className="flex h-48 flex-col justify-between pb-5 text-right">
          {ticks.map((tick) => (
            <span key={tick} className="font-body text-[11px] leading-none text-text-muted">
              {formatTokens(tick)}
            </span>
          ))}
        </div>
        <div className="relative h-48 min-w-0 flex-1">
          <div aria-hidden className="absolute inset-x-0 top-0 bottom-5 flex flex-col justify-between">
            {ticks.map((tick) => (
              <div key={tick} className="h-px" style={{ backgroundColor: 'var(--chart-grid)' }} />
            ))}
          </div>
          <div className="absolute inset-0 flex items-stretch gap-0.5">
            {days.map((day, i) => {
              const total = totals[i];
              const showLabel = i === 0 || i === days.length - 1 || i % labelEvery === 0;
              return (
                <div
                  key={day.day}
                  className="relative flex min-w-0 flex-1 flex-col items-center"
                  onMouseEnter={() => setActive(i)}
                  onMouseLeave={() => setActive((current) => (current === i ? null : current))}
                >
                  <button
                    type="button"
                    className="flex w-full flex-1 flex-col justify-end pb-0 focus:outline-none focus-visible:ring-2 focus-visible:ring-accent"
                    aria-label={`${shortDay(day.day)}: ${total.toLocaleString()} tokens`}
                    onFocus={() => setActive(i)}
                    onBlur={() => setActive((current) => (current === i ? null : current))}
                  >
                    <div
                      className="mx-auto flex w-full max-w-6 flex-col-reverse gap-0.5 overflow-hidden rounded-t"
                      style={{ height: `${(total / max) * 100}%` }}
                    >
                      {stacks[i]
                        .filter((segment) => segment.value > 0)
                        .map((segment) => (
                          <div
                            key={segment.key}
                            style={{
                              backgroundColor: colorOf(segment.key, keys),
                              flexGrow: segment.value,
                              flexBasis: 0,
                              minHeight: 1,
                            }}
                          />
                        ))}
                    </div>
                  </button>
                  <span className="h-5 whitespace-nowrap pt-1 font-body text-[11px] leading-none text-text-muted">
                    {showLabel ? shortDay(day.day) : ''}
                  </span>
                  {active === i ? (
                    <div
                      role="tooltip"
                      className={`absolute bottom-full z-10 mb-1 w-max max-w-64 rounded-md border border-border bg-surface-raised px-3 py-2 shadow-card ${
                        i > days.length / 2 ? 'right-0' : 'left-0'
                      }`}
                    >
                      <p className="font-body text-xs font-semibold text-bark-900">
                        {shortDay(day.day)} · {formatTokens(total)}
                      </p>
                      <ul className="mt-1 space-y-0.5">
                        {stacks[i]
                          .filter((segment) => segment.value > 0)
                          .reverse()
                          .map((segment) => (
                            <li
                              key={segment.key}
                              className="flex items-center gap-1.5 font-body text-xs text-text-secondary"
                            >
                              <span
                                aria-hidden
                                className="size-2 rounded-sm"
                                style={{ backgroundColor: colorOf(segment.key, keys) }}
                              />
                              <span className="min-w-0 truncate">{nameOf(segment.key)}</span>
                              <span className="ml-auto pl-3 tabular-nums">
                                {formatTokens(segment.value)}
                              </span>
                            </li>
                          ))}
                      </ul>
                    </div>
                  ) : null}
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </figure>
  );
}
