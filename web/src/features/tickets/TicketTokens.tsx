import {
  addUsage,
  formatTokens,
  partialRunsNote,
  tokenDetail,
  type UsageAmount,
  usageSummary,
  usageTotal,
} from '../runs/tokenUsage';
import { useTicketUsage } from './useTicketUsage';

interface TicketTokensProps {
  ticketId: string;
  onViewRuns?: () => void;
}

function plural(n: number, word: string): string {
  return `${n} ${word}${n === 1 ? '' : 's'}`;
}

function Line({ label, amount }: { label?: string; amount: UsageAmount }) {
  const total = usageTotal(amount);
  const summary = usageSummary(amount);
  return (
    <p
      className="font-body text-xs text-text-secondary"
      title={tokenDetail(amount.tokens, amount.subagentTokens)}
    >
      {label && <span className="text-text-muted">{label}: </span>}
      <span className="font-medium text-text-primary">
        {total == null ? 'not reported' : formatTokens(total)}
      </span>
      {summary && <span> ({summary})</span>}
    </p>
  );
}

/**
 * The ticket's token total on the Detail tab, so a finished ticket shows what it
 * spent without opening Runs. Nothing renders until some run reported usage.
 */
export function TicketTokens({ ticketId, onViewRuns }: TicketTokensProps) {
  const { data: usage } = useTicketUsage(ticketId);
  if (!usage) return null;

  const subtickets = usage.subtickets;
  const subticketsReported = subtickets != null && subtickets.runsWithUsage > 0;
  if (usage.own.runsWithUsage === 0 && !subticketsReported) return null;
  const partialRuns = usage.own.partialRuns + (subticketsReported ? subtickets.partialRuns : 0);

  return (
    <dl className="space-y-1 rounded-md border border-border bg-surface px-3 py-3">
      <dt className="font-body text-xs font-medium text-text-muted">Tokens</dt>
      <dd>
        <Line label={subticketsReported ? 'This ticket' : undefined} amount={usage.own} />
      </dd>
      {subticketsReported && (
        <dd>
          <Line
            label={`Incl. ${plural(subtickets.ticketCount, 'subticket')}`}
            amount={addUsage(usage.own, subtickets)}
          />
        </dd>
      )}
      {partialRuns > 0 && (
        <dd className="font-body text-xs text-text-muted">{partialRunsNote(partialRuns)}</dd>
      )}
      {onViewRuns && (
        <dd>
          <button
            type="button"
            onClick={onViewRuns}
            className="font-body text-xs font-medium text-moss-700 underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2"
          >
            View runs →
          </button>
        </dd>
      )}
    </dl>
  );
}
