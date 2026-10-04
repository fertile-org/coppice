import {
  Check,
  Clock3,
  ExternalLink,
  FileClock,
  GitBranch,
  Pencil,
  X,
} from 'lucide-react';
import { useState, type ReactNode } from 'react';
import { Button } from '../../components/ui/button';
import { parseApiErrorMessage } from '../../lib/api';
import type { KnowledgeItem, SimilarNeighbor } from '../../lib/schemas/knowledge';
import { guidanceForType } from './curationGuide';
import {
  TYPE_LABELS,
  duplicateRejectReason,
  formatDate,
  humanize,
  scopeLabel,
  shortId,
  statusPillClass,
} from './knowledgeFormat';
import {
  fetchKnowledgeItem,
  useApproveKnowledge,
  useExpireKnowledge,
  useMarkKnowledgeStale,
  useRejectKnowledge,
  useSimilarKnowledge,
  useSupersedeKnowledge,
} from './useKnowledge';

function NearDuplicateAssist({
  item,
  canGovern,
  busy,
  onOpenNeighbor,
  onApproveAnyway,
  onRejectDuplicate,
  onStartSupersede,
}: {
  item: KnowledgeItem;
  canGovern: boolean;
  busy: boolean;
  onOpenNeighbor: (neighborId: string) => void;
  onApproveAnyway: () => void;
  onRejectDuplicate: (neighbor: SimilarNeighbor) => void;
  onStartSupersede: (neighbor: SimilarNeighbor) => void;
}) {
  const similar = useSimilarKnowledge(item.id, item.status === 'pending');
  const neighbors = similar.data?.items ?? [];

  return (
    <section
      aria-label="Near-duplicate assist"
      className="mt-4 rounded-lg border border-moss-200 bg-moss-50/70 p-4"
    >
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h3 className="font-display text-sm font-semibold text-bark-900">
            Near duplicates
          </h3>
          <p className="mt-0.5 font-body text-xs text-text-secondary">
            Close approved neighbors before you confirm this pending item.
          </p>
        </div>
        {canGovern && (
          <Button type="button" size="sm" disabled={busy} onClick={onApproveAnyway}>
            <Check className="size-3.5" aria-hidden="true" />
            Approve anyway
          </Button>
        )}
      </div>

      {similar.isLoading && (
        <p className="mt-3 font-body text-xs text-text-muted">
          Checking for close matches…
        </p>
      )}
      {similar.isError && (
        <p className="mt-3 font-body text-xs text-danger">
          Unable to load similar knowledge.
        </p>
      )}
      {!similar.isLoading && !similar.isError && neighbors.length === 0 && (
        <p className="mt-3 font-body text-sm text-text-secondary">No close matches</p>
      )}
      {neighbors.length > 0 && (
        <ul className="mt-3 space-y-3">
          {neighbors.map((neighbor) => (
            <li
              key={neighbor.itemId}
              className="rounded-md border border-border bg-surface-raised px-3 py-2"
            >
              <div className="flex flex-wrap items-start justify-between gap-2">
                <div className="min-w-0">
                  <p className="font-body text-sm font-medium text-bark-900">
                    {neighbor.title}
                  </p>
                  <p className="mt-0.5 font-body text-xs text-text-muted">
                    {TYPE_LABELS[neighbor.knowledgeType] ??
                      humanize(neighbor.knowledgeType)}{' '}
                    · {humanize(neighbor.scope)} · match {neighbor.score.toFixed(3)}
                  </p>
                </div>
                <button
                  type="button"
                  onClick={() => onOpenNeighbor(neighbor.itemId)}
                  className="inline-flex items-center gap-1 font-body text-xs font-medium text-moss-700 hover:underline"
                >
                  Open
                  <ExternalLink className="size-3" aria-hidden="true" />
                </button>
              </div>
              {canGovern && (
                <div className="mt-2 flex flex-wrap gap-2">
                  <Button
                    type="button"
                    variant="destructive"
                    size="sm"
                    disabled={busy}
                    onClick={() => onRejectDuplicate(neighbor)}
                  >
                    Reject as duplicate
                  </Button>
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    disabled={busy}
                    onClick={() => onStartSupersede(neighbor)}
                  >
                    Start supersede
                  </Button>
                </div>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function Meta({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="min-w-0">
      <dt className="font-body text-xs uppercase tracking-wide text-text-muted">{label}</dt>
      <dd className="mt-0.5 font-body text-sm text-text-secondary">{children}</dd>
    </div>
  );
}

export function KnowledgeCard({
  item,
  canGovern,
  focused,
  onOpenTicket,
  onOpenNeighbor,
  onEdit,
  onSupersede,
  onReject,
}: {
  item: KnowledgeItem;
  canGovern: boolean;
  focused?: boolean;
  onOpenTicket: (ticketId: string) => void | Promise<void>;
  onOpenNeighbor: (neighborId: string) => void;
  onEdit: (item: KnowledgeItem) => void;
  onSupersede: (item: KnowledgeItem) => void;
  onReject: (item: KnowledgeItem) => void;
}) {
  const approve = useApproveKnowledge();
  const reject = useRejectKnowledge();
  const supersede = useSupersedeKnowledge();
  const markStale = useMarkKnowledgeStale();
  const expire = useExpireKnowledge();
  const [error, setError] = useState<string | null>(null);
  const typeGuidance = guidanceForType(item.knowledgeType);

  const busy =
    approve.isPending ||
    reject.isPending ||
    supersede.isPending ||
    markStale.isPending ||
    expire.isPending;

  const hasExpiry = item.expiresAt !== null;
  const sourceCanOpen =
    (item.sourceType === 'ticket' || item.sourceType === 'agent_summary') &&
    item.sourceId;
  const versionArgs = { id: item.id, expectedVersion: item.version };

  async function run(action: () => Promise<unknown>) {
    setError(null);
    try {
      await action();
    } catch (cause) {
      setError(
        parseApiErrorMessage(
          cause,
          'Unable to update this knowledge item. Refresh and try again.',
        ),
      );
    }
  }

  async function rejectAsDuplicate(neighbor: SimilarNeighbor) {
    await run(() =>
      reject.mutateAsync({
        ...versionArgs,
        reason: duplicateRejectReason(neighbor.title, neighbor.itemId),
      }),
    );
  }

  async function startSupersede(neighbor: SimilarNeighbor) {
    await run(async () => {
      const neighborItem = await fetchKnowledgeItem(neighbor.itemId);
      await supersede.mutateAsync({
        id: neighbor.itemId,
        expectedVersion: neighborItem.version,
        replacement: {
          scope: item.scope,
          boardId: item.boardId,
          agentId: item.agentId,
          knowledgeType: item.knowledgeType,
          title: item.title,
          content: item.content,
          sourceType: item.sourceType,
          sourceId: item.sourceId,
          sourceRunId: item.sourceRunId,
          confidence: item.confidence,
        },
      });
      await reject.mutateAsync({
        ...versionArgs,
        reason: duplicateRejectReason(neighbor.title, neighbor.itemId),
      });
    });
  }

  const showActions = canGovern && !item.supersededBy;

  return (
    <article
      id={`knowledge-item-${item.id}`}
      data-focused={focused ? 'true' : undefined}
      className={[
        'rounded-xl border bg-surface-raised shadow-card',
        focused ? 'border-moss-400 ring-2 ring-moss-200' : 'border-border',
      ].join(' ')}
    >
      <div className="grid gap-5 p-5 lg:grid-cols-[minmax(0,1fr)_15rem]">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <span className={statusPillClass(item.status)}>{humanize(item.status)}</span>
            <span className="font-body text-xs font-medium uppercase tracking-wide text-moss-700">
              {TYPE_LABELS[item.knowledgeType]}
            </span>
            <span className="font-body text-xs text-text-muted">
              {humanize(item.confidence)} confidence
            </span>
          </div>
          <h2 className="mt-2 font-display text-lg font-semibold text-bark-900">
            {item.title}
          </h2>
          <p className="mt-1.5 whitespace-pre-wrap font-body text-sm leading-relaxed text-text-secondary">
            {item.content}
          </p>

          {item.sourceTicketIds.length > 0 && (
            <div
              data-testid="compaction-provenance"
              className="mt-3 flex flex-wrap items-center gap-x-2 gap-y-1 font-body text-xs text-text-secondary"
            >
              <span className="font-medium text-text-primary">
                From {item.sourceTicketIds.length}{' '}
                {item.sourceTicketIds.length === 1 ? 'ticket' : 'tickets'}
              </span>
              {item.sourceTicketIds.map((ticketId) => (
                <button
                  key={ticketId}
                  type="button"
                  onClick={() => void onOpenTicket(ticketId)}
                  className="inline-flex items-center gap-1 rounded-md bg-paper-100 px-1.5 py-0.5 font-mono text-xs text-moss-700 hover:underline"
                  aria-label={`Open source ticket ${shortId(ticketId)}`}
                >
                  {shortId(ticketId)}
                  <ExternalLink className="size-3" aria-hidden="true" />
                </button>
              ))}
              {item.compactionAgentName && (
                <span>· Proposed by {item.compactionAgentName}</span>
              )}
            </div>
          )}

          {item.status === 'pending' && (
            <p
              data-testid="pending-type-guidance"
              className="mt-3 font-body text-xs leading-relaxed text-text-secondary"
            >
              <span className="font-medium text-moss-800">
                {typeGuidance.approveExample}
              </span>
              <span className="mx-1.5 text-text-muted" aria-hidden="true">
                ·
              </span>
              <span className="font-medium text-danger">{typeGuidance.rejectExample}</span>
            </p>
          )}

          {(item.policyReason || item.rejectionReason) && (
            <div className="mt-3 space-y-1 rounded-md border border-border bg-paper-50 px-3 py-2 font-body text-xs text-text-secondary">
              {item.policyReason && (
                <p>
                  <span className="font-medium">Policy:</span> {item.policyReason}
                </p>
              )}
              {item.rejectionReason && (
                <p>
                  <span className="font-medium">Rejected:</span> {item.rejectionReason}
                </p>
              )}
            </div>
          )}

          {item.status === 'pending' && (
            <NearDuplicateAssist
              item={item}
              canGovern={canGovern}
              busy={busy}
              onOpenNeighbor={onOpenNeighbor}
              onApproveAnyway={() => void run(() => approve.mutateAsync(versionArgs))}
              onRejectDuplicate={(neighbor) => void rejectAsDuplicate(neighbor)}
              onStartSupersede={(neighbor) => void startSupersede(neighbor)}
            />
          )}
        </div>

        <div className="border-t border-border pt-4 lg:border-l lg:border-t-0 lg:pl-5 lg:pt-0">
          <dl className="grid gap-x-5 gap-y-3 sm:grid-cols-2 lg:grid-cols-1">
            <Meta label="Scope">{scopeLabel(item)}</Meta>
            <Meta label="Source">
              <span className="flex items-center gap-2">
                {humanize(item.sourceType)}
                {sourceCanOpen && (
                  <button
                    type="button"
                    onClick={() => void onOpenTicket(item.sourceId!)}
                    className="inline-flex items-center gap-1 text-xs font-medium text-moss-700 hover:underline"
                  >
                    Open
                    <ExternalLink className="size-3" aria-hidden="true" />
                  </button>
                )}
              </span>
            </Meta>
            <Meta label="Revision">
              {item.revisionNumber} · version {item.version}
            </Meta>
            <Meta label="Usage">
              {item.usageCount} runs · last {formatDate(item.lastUsedAt)}
            </Meta>
            <Meta label="Expiry">{formatDate(item.expiresAt)}</Meta>
            <Meta label="Updated">{formatDate(item.updatedAt)}</Meta>
          </dl>
          {(item.supersedesItemId || item.supersededBy || item.sourceRunId) && (
            <div className="mt-3 flex flex-col gap-1 rounded-md bg-paper-100 px-3 py-2 font-mono text-xs text-text-secondary">
              {item.supersedesItemId && (
                <span>Supersedes {shortId(item.supersedesItemId)}</span>
              )}
              {item.supersededBy && <span>Superseded by {shortId(item.supersededBy)}</span>}
              {item.sourceRunId && <span>Source run {shortId(item.sourceRunId)}</span>}
            </div>
          )}
        </div>
      </div>

      {showActions && (
        <div className="flex flex-wrap gap-2 border-t border-border px-5 py-3">
          {item.status !== 'approved' && item.status !== 'pending' && (
            <Button
              type="button"
              size="sm"
              loading={approve.isPending}
              disabled={busy}
              onClick={() => void run(() => approve.mutateAsync(versionArgs))}
            >
              <Check className="size-3.5" aria-hidden="true" />
              Approve
            </Button>
          )}
          <Button
            type="button"
            variant="secondary"
            size="sm"
            disabled={busy}
            onClick={() => onEdit(item)}
          >
            <Pencil className="size-3.5" aria-hidden="true" />
            Edit
          </Button>
          {item.status === 'pending' && (
            <Button
              type="button"
              variant="destructive"
              size="sm"
              disabled={busy}
              onClick={() => onReject(item)}
            >
              <X className="size-3.5" aria-hidden="true" />
              Reject
            </Button>
          )}
          {(item.status === 'approved' || item.status === 'stale') && (
            <Button
              type="button"
              variant="secondary"
              size="sm"
              disabled={busy}
              onClick={() => onSupersede(item)}
            >
              <GitBranch className="size-3.5" aria-hidden="true" />
              Supersede
            </Button>
          )}
          {item.status === 'approved' && (
            <Button
              type="button"
              variant="secondary"
              size="sm"
              disabled={busy}
              onClick={() => void run(() => markStale.mutateAsync(versionArgs))}
            >
              <FileClock className="size-3.5" aria-hidden="true" />
              Mark stale
            </Button>
          )}
          {!hasExpiry && (
            <Button
              type="button"
              variant="secondary"
              size="sm"
              disabled={busy}
              onClick={() => void run(() => expire.mutateAsync(versionArgs))}
            >
              <Clock3 className="size-3.5" aria-hidden="true" />
              Expire now
            </Button>
          )}
        </div>
      )}

      {error && (
        <p
          role="alert"
          className="mx-5 mb-4 rounded-md bg-danger-muted px-3 py-2 font-body text-sm text-danger"
        >
          {error}
        </p>
      )}
    </article>
  );
}
