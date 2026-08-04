import { BookOpen, ChevronDown, ExternalLink } from 'lucide-react';
import { useKnowledgeUsed } from './useKnowledge';

interface KnowledgeUsedProps {
  runId: string;
  enabled: boolean;
  onOpenTicket: (ticketId: string) => void | Promise<void>;
}

function humanize(value: string): string {
  return value
    .split('_')
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ');
}

export function KnowledgeUsed({
  runId,
  enabled,
  onOpenTicket,
}: KnowledgeUsedProps) {
  const { data, isLoading, isError } = useKnowledgeUsed(runId, enabled);

  if (!enabled) return null;

  return (
    <section
      aria-label="Knowledge Used"
      className="mt-4 border-t border-border pt-3"
    >
      <div className="flex items-center gap-2">
        <BookOpen className="size-4 text-moss-600" aria-hidden="true" />
        <h4 className="font-display text-sm font-semibold text-bark-800">
          Knowledge Used
        </h4>
        {data && data.length > 0 && (
          <span className="rounded-full bg-moss-100 px-2 py-0.5 font-body text-xs text-moss-800">
            {data.length}
          </span>
        )}
      </div>

      {isLoading && (
        <p className="mt-2 font-body text-xs text-text-muted">
          Loading knowledge audit…
        </p>
      )}
      {isError && (
        <p className="mt-2 font-body text-xs text-danger">
          Unable to load the knowledge audit.
        </p>
      )}
      {!isLoading && !isError && data?.length === 0 && (
        <p className="mt-2 font-body text-xs text-text-muted">
          This run did not include stored knowledge.
        </p>
      )}

      {data && data.length > 0 && (
        <ol className="mt-2 space-y-2">
          {data.map((usage) => {
            const sourceOpensTicket =
              usage.sourceType === 'ticket' ||
              usage.sourceType === 'agent_summary';
            return (
              <li
                key={usage.revisionId}
                className="rounded-md border border-moss-200 bg-moss-50/70 px-3 py-2"
              >
                <div className="flex flex-wrap items-start justify-between gap-2">
                  <div>
                    <p className="font-body text-sm font-medium text-bark-900">
                      {usage.rank}. {usage.title}
                    </p>
                    <p className="mt-0.5 font-body text-xs text-text-muted">
                      {humanize(usage.knowledgeType)} · {humanize(usage.scope)} ·{' '}
                      {usage.tokenCount} tokens · similarity{' '}
                      {usage.similarity.toFixed(3)}
                    </p>
                    <p className="mt-1 font-body text-xs text-text-secondary">
                      Revision{' '}
                      <code className="font-mono text-bark-700">
                        {usage.revisionId}
                      </code>
                    </p>
                    <p className="mt-0.5 font-body text-xs text-text-secondary">
                      Source {humanize(usage.sourceType)} ·{' '}
                      {usage.sourceId ? (
                        <code className="font-mono text-bark-700">
                          {usage.sourceId}
                        </code>
                      ) : (
                        'no source ID'
                      )}
                    </p>
                  </div>
                  {sourceOpensTicket && usage.sourceId && (
                    <button
                      type="button"
                      onClick={() => void onOpenTicket(usage.sourceId!)}
                      className="inline-flex items-center gap-1 font-body text-xs font-medium text-moss-700 hover:underline"
                    >
                      Source ticket
                      <ExternalLink className="size-3" aria-hidden="true" />
                    </button>
                  )}
                </div>
                <details className="group mt-2">
                  <summary className="flex cursor-pointer list-none items-center gap-1 font-body text-xs font-medium text-text-secondary">
                    <ChevronDown
                      className="size-3 transition-transform group-open:rotate-180"
                      aria-hidden="true"
                    />
                    Exact rendered revision
                  </summary>
                  <pre className="mt-2 max-h-64 overflow-auto rounded-md bg-paper-100 p-2 font-mono text-xs text-bark-700 whitespace-pre-wrap">
                    {usage.renderedContent}
                  </pre>
                </details>
              </li>
            );
          })}
        </ol>
      )}
    </section>
  );
}
