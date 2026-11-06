import { Bot, Coins } from 'lucide-react';
import type { RequestUsage, SubagentUsage, TokenCounts } from '../../lib/schemas/usage';
import { formatTokens, tokenDetail, tokenSummary, USAGE_COPY } from './tokenUsage';
import { useRunUsage } from './useRunUsage';

interface RunTokenUsageProps {
  runId: string;
  enabled: boolean;
  /** Poll while the run is still going. */
  live?: boolean;
}

function Tokens({ tokens }: { tokens: TokenCounts }) {
  const summary = tokenSummary(tokens);
  return (
    <span className="font-body text-xs text-text-secondary" title={tokenDetail(tokens)}>
      {summary ?? USAGE_COPY.notReported}
    </span>
  );
}

function RequestRow({ request, index }: { request: RequestUsage; index: number }) {
  return (
    <li
      aria-label={`${request.granularity === 'turn' ? 'Turn' : 'Request'} ${index + 1}`}
      className="flex flex-wrap items-center gap-2 rounded-md border border-border bg-paper-50 px-3 py-1.5"
    >
      <span className="font-mono text-xs text-text-muted">#{index + 1}</span>
      {request.model && (
        <code className="font-mono text-xs text-bark-900">{request.model}</code>
      )}
      <span className="ml-auto">
        <Tokens tokens={request.tokens} />
      </span>
    </li>
  );
}

function SubagentRow({ subagent }: { subagent: SubagentUsage }) {
  const summary = tokenSummary(subagent.tokens);
  return (
    <li
      aria-label={`Subagent ${subagent.agentType ?? subagent.id}`}
      className="rounded-md border border-border bg-paper-50 px-3 py-2"
    >
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-body text-xs font-medium text-bark-900">
          {subagent.agentType ?? 'Subagent'}
        </span>
        {subagent.model && (
          <code className="font-mono text-xs text-text-secondary">{subagent.model}</code>
        )}
        {subagent.status && (
          <span className="rounded-full bg-moss-100 px-2 py-0.5 font-body text-xs text-moss-800">
            {subagent.status}
          </span>
        )}
        <span className="ml-auto font-body text-xs text-text-secondary" title={tokenDetail(subagent.tokens)}>
          {summary ??
            (subagent.totalTokens != null
              ? `${formatTokens(subagent.totalTokens)} total`
              : USAGE_COPY.notReported)}
        </span>
      </div>
      {subagent.description && (
        <p className="mt-1 font-body text-xs text-text-muted">{subagent.description}</p>
      )}
      {subagent.toolUses != null && (
        <p className="mt-1 font-body text-xs text-text-muted">
          {subagent.toolUses} tool {subagent.toolUses === 1 ? 'use' : 'uses'}
        </p>
      )}
    </li>
  );
}

export function RunTokenUsage({ runId, enabled, live = false }: RunTokenUsageProps) {
  const { data, isLoading, isError } = useRunUsage(runId, enabled, live ? 3000 : false);

  if (!enabled) return null;

  return (
    <section aria-label="Tokens" className="space-y-3">
      {isLoading && <p className="font-body text-xs text-text-muted">Loading token usage…</p>}
      {isError && <p className="font-body text-xs text-danger">Unable to load token usage.</p>}
      {data && !data.totals && (
        <p className="font-body text-xs text-text-muted">{USAGE_COPY.noUsage}</p>
      )}

      {data?.totals && (
        <div>
          <div className="flex flex-wrap items-center gap-2">
            <Coins className="size-4 text-moss-600" aria-hidden="true" />
            <h4 className="font-display text-sm font-semibold text-bark-800">Run total</h4>
            <Tokens tokens={data.totals.tokens} />
          </div>
          {data.totals.status === 'partial' && (
            <p className="mt-1 font-body text-xs text-text-muted">{USAGE_COPY.partial}</p>
          )}
        </div>
      )}

      {data && data.requests.length > 0 && (
        <div>
          <h4 className="font-display text-sm font-semibold text-bark-800">
            Requests{' '}
            <span className="rounded-full bg-moss-100 px-2 py-0.5 font-body text-xs font-normal text-moss-800">
              {data.requests.length}
            </span>
          </h4>
          <ol className="mt-2 space-y-1">
            {data.requests.map((request, index) => (
              <RequestRow key={request.id ?? index} request={request} index={index} />
            ))}
          </ol>
        </div>
      )}

      {data && data.subagents.length > 0 && (
        <div>
          <div className="flex items-center gap-2">
            <Bot className="size-4 text-moss-600" aria-hidden="true" />
            <h4 className="font-display text-sm font-semibold text-bark-800">Subagents</h4>
            <span className="rounded-full bg-moss-100 px-2 py-0.5 font-body text-xs text-moss-800">
              {data.subagents.length}
            </span>
          </div>
          <ul className="mt-2 space-y-1.5">
            {data.subagents.map((subagent) => (
              <SubagentRow key={subagent.id} subagent={subagent} />
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}
