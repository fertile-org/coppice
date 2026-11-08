import { Coins } from 'lucide-react';
import type { TicketUsage, UsageGroup } from '../../lib/schemas/usage';
import {
  addUsage,
  formatTokens,
  partialRunsNote,
  tokenDetail,
  type UsageAmount,
  usageSummary,
  usageTotal,
} from '../runs/tokenUsage';

interface TicketUsageSummaryProps {
  usage: TicketUsage;
  agentName: (agentId: string) => string;
}

function plural(n: number, word: string): string {
  return `${n} ${word}${n === 1 ? '' : 's'}`;
}

function Total({ label, amount }: { label: string; amount: UsageAmount }) {
  const total = usageTotal(amount);
  const summary = usageSummary(amount);
  return (
    <div
      className="flex flex-wrap items-baseline gap-x-2"
      title={tokenDetail(amount.tokens, amount.subagentTokens)}
    >
      <dt className="font-body text-xs text-text-muted">{label}</dt>
      <dd className="font-body text-sm font-medium text-text-primary">
        {total == null ? 'not reported' : formatTokens(total)}
      </dd>
      {summary && <dd className="font-body text-xs text-text-secondary">{summary}</dd>}
    </div>
  );
}

function AgentRow({ group, name }: { group: UsageGroup; name: string }) {
  const total = usageTotal(group);
  return (
    <li
      className="flex flex-wrap items-center gap-2 font-body text-xs"
      title={tokenDetail(group.tokens, group.subagentTokens)}
    >
      <span className="font-medium text-text-primary">{name}</span>
      <span className="text-text-muted">×{group.runs}</span>
      {group.runsNotReported > 0 && (
        <span className="text-text-muted">({group.runsNotReported} not reported)</span>
      )}
      <span className="ml-auto text-text-secondary">
        {total == null ? 'not reported' : formatTokens(total)}
      </span>
    </li>
  );
}

/**
 * Tokens spent on the ticket, by agent; split children are added separately.
 * Only runs Coppice could read are counted.
 */
export function TicketUsageSummary({ usage, agentName }: TicketUsageSummaryProps) {
  const subtickets = usage.subtickets;
  const hasSubticketUsage = (subtickets?.runsWithUsage ?? 0) > 0;
  if (usage.own.runsWithUsage === 0 && !hasSubticketUsage) return null;

  const including = subtickets && hasSubticketUsage ? addUsage(usage.own, subtickets) : null;
  const partialRuns = usage.own.partialRuns + (hasSubticketUsage ? (subtickets?.partialRuns ?? 0) : 0);

  return (
    <section
      aria-label="Token usage"
      className="rounded-md border border-border bg-surface px-4 py-3"
    >
      <div className="flex items-center gap-2">
        <Coins className="size-4 text-moss-600" aria-hidden="true" />
        <h3 className="font-display text-sm font-semibold text-bark-800">Tokens</h3>
      </div>
      <dl className="mt-2 space-y-1">
        <Total label="This ticket" amount={usage.own} />
        {including && subtickets && (
          <Total
            label={`Including ${plural(subtickets.ticketCount, 'subticket')}`}
            amount={including}
          />
        )}
      </dl>
      {usage.own.runsNotReported > 0 && (
        <p className="mt-1 font-body text-xs text-text-muted">
          {plural(usage.own.runsNotReported, 'run')} reported no tokens.
        </p>
      )}
      {partialRuns > 0 && (
        <p className="mt-1 font-body text-xs text-text-muted">{partialRunsNote(partialRuns)}</p>
      )}
      {usage.byAgent.length > 1 && (
        <ul aria-label="Tokens by agent" className="mt-2 space-y-1 border-t border-border pt-2">
          {usage.byAgent.map((group) => (
            <AgentRow key={group.key} group={group} name={agentName(group.key)} />
          ))}
        </ul>
      )}
    </section>
  );
}
