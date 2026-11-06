import { Coins } from 'lucide-react';
import type { TicketUsage, TokenCounts, UsageGroup } from '../../lib/schemas/usage';
import {
  addTokens,
  formatTokens,
  tokenDetail,
  tokenSummary,
  totalTokens,
} from '../runs/tokenUsage';

interface TicketUsageSummaryProps {
  usage: TicketUsage;
  agentName: (agentId: string) => string;
}

function plural(n: number, word: string): string {
  return `${n} ${word}${n === 1 ? '' : 's'}`;
}

function Total({ label, tokens }: { label: string; tokens: TokenCounts }) {
  const total = totalTokens(tokens);
  return (
    <div className="flex flex-wrap items-baseline gap-x-2" title={tokenDetail(tokens)}>
      <dt className="font-body text-xs text-text-muted">{label}</dt>
      <dd className="font-body text-sm font-medium text-text-primary">
        {total == null ? 'not reported' : formatTokens(total)}
      </dd>
      {tokenSummary(tokens) && (
        <dd className="font-body text-xs text-text-secondary">{tokenSummary(tokens)}</dd>
      )}
    </div>
  );
}

function AgentRow({ group, name }: { group: UsageGroup; name: string }) {
  const total = totalTokens(group.tokens);
  return (
    <li className="flex flex-wrap items-center gap-2 font-body text-xs" title={tokenDetail(group.tokens)}>
      <span className="font-medium text-text-primary">{name}</span>
      <span className="text-text-muted">×{group.runs}</span>
      {group.runsWithUsage < group.runs && (
        <span className="text-text-muted">({group.runsWithUsage} reported)</span>
      )}
      <span className="ml-auto text-text-secondary">
        {total == null ? 'not reported' : formatTokens(total)}
      </span>
    </li>
  );
}

/** Tokens spent on the ticket, by agent; split children are added separately. */
export function TicketUsageSummary({ usage, agentName }: TicketUsageSummaryProps) {
  const subtickets = usage.subtickets;
  const hasSubticketUsage = (subtickets?.runsWithUsage ?? 0) > 0;
  if (usage.own.runsWithUsage === 0 && !hasSubticketUsage) return null;

  const including =
    subtickets && hasSubticketUsage ? addTokens(usage.own.tokens, subtickets.tokens) : null;

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
        <Total label="This ticket" tokens={usage.own.tokens} />
        {including && subtickets && (
          <Total
            label={`Including ${plural(subtickets.ticketCount, 'subticket')}`}
            tokens={including}
          />
        )}
      </dl>
      {usage.own.runsWithUsage < usage.own.runs && (
        <p className="mt-1 font-body text-xs text-text-muted">
          {plural(usage.own.runs - usage.own.runsWithUsage, 'run')} reported no tokens.
        </p>
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
