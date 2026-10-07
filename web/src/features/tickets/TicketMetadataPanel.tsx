import { useEffect, useState } from 'react';
import { BOARD_COLUMNS, type TicketStatus } from '../board/columns';
import type { HumanReview, Ticket } from '../board/useTickets';
import { useToast } from '../../components/ToastProvider';
import { Button } from '../../components/ui/button';
import { Combobox } from '../../components/ui/combobox';
import { Input } from '../../components/ui/input';
import { Label } from '../../components/ui/label';
import {
  SUBSTATUSES,
  SUBSTATUS_LABELS,
  substatusMetadataSchema,
  substatusOptionalReason,
  substatusRequiresMetadata,
  type Substatus,
} from '../../lib/schemas/substatus';
import { ticketPrioritySchema } from '../../lib/schemas/ticket';
import { useRepos } from '../repos/useRepos';
import { useAgentRuns } from './useAgentRuns';
import { TicketStatusBadge } from './TicketStatusBadge';
import { TicketGitActions } from './TicketGitActions';
import {
  useAgents,
  useApproveSplits,
  useAssignAgent,
  useDismissSplits,
  useTicketGitInfo,
  useUpdateTicket,
  useUpdateTicketStatus,
} from './useTicket';

interface TicketMetadataPanelProps {
  ticket: Ticket;
}

function buildCodeReviewUrl(
  ticket: Ticket,
  repoId: string,
  worktreePath: string | null,
) {
  const params = new URLSearchParams({ repoId, ticketId: ticket.id });
  if (worktreePath) params.set('worktree', worktreePath);
  return `/code?${params.toString()}`;
}

function ReviewedCommit({ review }: { review: HumanReview }) {
  return (
    <div className="space-y-1">
      <p className="font-body text-xs text-text-muted">
        Reviewed at <code className="font-mono">{review.shortSha}</code>
      </p>
      {review.stale && (
        <p className="font-body text-sm text-text-secondary">
          New commits since your review. Review again before you accept.
        </p>
      )}
    </div>
  );
}

function metadataFromTicket(ticket: Ticket): Record<string, unknown> {
  return ticket.substatusMetadata ?? {};
}

export function TicketMetadataPanel({ ticket }: TicketMetadataPanelProps) {
  const toast = useToast();
  const updateTicket = useUpdateTicket(ticket.id);
  const updateStatus = useUpdateTicketStatus(ticket.id);
  const assignAgent = useAssignAgent(ticket.id);
  const approveSplits = useApproveSplits(ticket.id);
  const dismissSplits = useDismissSplits(ticket.id);
  const { data: agents } = useAgents();
  const { data: repos } = useRepos();
  const { data: runs } = useAgentRuns(ticket.id);
  const latestRunWorktreePath = runs?.[0]?.worktreePath ?? null;
  const selectedRepo = repos?.find((repo) => repo.id === ticket.repoId);
  const repoNotReady = Boolean(
    ticket.repoId && selectedRepo && selectedRepo.verificationStatus !== 'ready',
  );
  const { data: gitInfo } = useTicketGitInfo(ticket.id, Boolean(ticket.repoId));
  const worktreePath =
    latestRunWorktreePath ??
    (gitInfo?.worktreeExists ? gitInfo.worktreePath : null);

  const [assigneeId, setAssigneeId] = useState(ticket.assigneeAgentId ?? '');
  const [assignError, setAssignError] = useState<string | null>(null);
  const [status, setStatus] = useState(ticket.status);
  const [substatus, setSubstatus] = useState<Substatus | ''>(
    (ticket.substatus as Substatus | undefined) ?? '',
  );
  const [metadata, setMetadata] = useState<Record<string, unknown>>(
    metadataFromTicket(ticket),
  );
  const [priority, setPriority] = useState(ticket.priority ?? '');
  const [repoId, setRepoId] = useState(ticket.repoId ?? '');
  const [error, setError] = useState<string | null>(null);
  const [splitError, setSplitError] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    setAssigneeId(ticket.assigneeAgentId ?? '');
    setStatus(ticket.status);
    setSubstatus((ticket.substatus as Substatus | undefined) ?? '');
    setMetadata(metadataFromTicket(ticket));
    setPriority(ticket.priority ?? '');
    setRepoId(ticket.repoId ?? '');
  }, [ticket]);

  async function handleApproveSplits() {
    const splits = ticket.pendingSplitRecommendation?.splits ?? [];
    if (splits.length === 0) return;

    const label = splits.length === 1 ? '1 child ticket' : `${splits.length} child tickets`;
    if (
      !window.confirm(
        `Create ${label} from this split recommendation? The parent ticket will remain unchanged.`,
      )
    ) {
      return;
    }

    setSplitError(null);
    try {
      await approveSplits.mutateAsync();
      toast.success('Child tickets created');
    } catch {
      setSplitError('Unable to approve splits.');
      toast.error('Unable to approve splits');
    }
  }

  async function handleDismissSplits() {
    setSplitError(null);
    try {
      await dismissSplits.mutateAsync();
      toast.success('Split recommendation dismissed');
    } catch {
      setSplitError('Unable to dismiss split recommendation.');
      toast.error('Unable to dismiss split recommendation');
    }
  }

  function updateMetadataField(key: string, value: string) {
    setMetadata((prev) => ({ ...prev, [key]: value }));
  }

  async function handleSave() {
    setError(null);
    setAssignError(null);

    const parsedPriority = priority
      ? ticketPrioritySchema.safeParse(priority)
      : { success: true as const, data: null };

    if (!parsedPriority.success) {
      setError('Invalid priority.');
      return;
    }

    if (substatus) {
      const validation = substatusMetadataSchema.safeParse({ substatus, metadata });
      if (!validation.success) {
        setError(validation.error.issues[0]?.message ?? 'Invalid substatus metadata.');
        return;
      }
    }

    const nextAssigneeId = assigneeId || null;
    const assigneeChanged =
      nextAssigneeId !== (ticket.assigneeAgentId ?? null);

    setIsSaving(true);
    try {
      if (assigneeChanged) {
        await assignAgent.mutateAsync(nextAssigneeId);
      }

      await updateStatus.mutateAsync({
        status,
        substatus: substatus || null,
        substatusMetadata: substatus ? metadata : undefined,
      });

      const nextRepoId = repoId || null;
      const repoChanged = nextRepoId !== (ticket.repoId ?? null);
      const priorityChanged = parsedPriority.data !== ticket.priority;

      if (repoChanged || priorityChanged) {
        await updateTicket.mutateAsync({
          ...(repoChanged ? { repoId: nextRepoId } : {}),
          ...(priorityChanged ? { priority: parsedPriority.data } : {}),
        });
      }

      toast.success('Metadata saved');
    } catch {
      setAssigneeId(ticket.assigneeAgentId ?? '');
      setError('Unable to save metadata.');
      setAssignError(assigneeChanged ? 'Unable to update assignee.' : null);
      toast.error('Unable to save metadata');
    } finally {
      setIsSaving(false);
    }
  }

  const isBusy =
    isSaving ||
    updateStatus.isPending ||
    updateTicket.isPending ||
    assignAgent.isPending ||
    approveSplits.isPending ||
    dismissSplits.isPending;
  const isArchived = Boolean(ticket.archivedAt);
  const formDisabled = isBusy || isArchived;
  const activeSubstatus = substatus || null;
  const assignedAgent = agents?.find((agent) => agent.id === assigneeId);

  return (
    <div className="space-y-5">
      {isArchived && (
        <p className="rounded-md border border-border bg-paper-200 px-3 py-2 font-body text-xs text-text-secondary">
          Unarchive this ticket to edit metadata, status, or assignment.
        </p>
      )}
      <div className="space-y-2">
        <Label htmlFor="ticket-assignee">Assignee</Label>
        <Combobox
          id="ticket-assignee"
          value={assigneeId}
          onValueChange={(value) => {
            setAssigneeId(value);
            setAssignError(null);
          }}
          disabled={formDisabled}
          placeholder="Unassigned"
          searchPlaceholder="Search agents…"
          clearable
          options={(agents ?? [])
            .filter((agent) => agent.enabled)
            .map((agent) => ({
              value: agent.id,
              label: `${agent.name} (${agent.role})`,
            }))}
        />
        {assignedAgent && (
          <p className="font-body text-xs text-text-muted">
            Assigned to {assignedAgent.name}
          </p>
        )}
        {ticket.pendingAssignRecommendation && (
          <p className="font-body text-sm text-text-muted">
            Recommends:{' '}
            <span className="inline-flex items-center rounded-full border border-border bg-surface px-2.5 py-0.5 font-medium text-text-primary">
              {ticket.pendingAssignRecommendation.recommendedAgentKey}
            </span>
          </p>
        )}
      </div>

      {ticket.pendingSplitRecommendation && (
        <div className="space-y-3 rounded-md border border-border bg-surface px-3 py-3">
          <p className="font-body text-xs font-medium text-text-muted">
            Pending split recommendation
          </p>
          <ul className="space-y-1">
            {ticket.pendingSplitRecommendation.splits.map((split, index) => (
              <li
                key={`${split.title}-${index}`}
                className="font-body text-sm text-text-primary"
              >
                {split.title}
              </li>
            ))}
          </ul>
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              onClick={() => void handleApproveSplits()}
              loading={approveSplits.isPending}
              disabled={formDisabled}
              className="flex-1"
            >
              {approveSplits.isPending ? 'Approving…' : 'Approve splits'}
            </Button>
            <Button
              type="button"
              variant="secondary"
              onClick={() => void handleDismissSplits()}
              loading={dismissSplits.isPending}
              disabled={formDisabled}
              className="flex-1"
            >
              {dismissSplits.isPending ? 'Dismissing…' : 'Dismiss'}
            </Button>
          </div>
        </div>
      )}

      {assignError && (
        <p className="rounded-md border border-danger-muted bg-danger-muted/40 px-3 py-2 font-body text-sm text-danger">
          {assignError}
        </p>
      )}

      {splitError && (
        <p className="rounded-md border border-danger-muted bg-danger-muted/40 px-3 py-2 font-body text-sm text-danger">
          {splitError}
        </p>
      )}

      {error && (
        <p className="rounded-md border border-danger-muted bg-danger-muted/40 px-3 py-2 font-body text-sm text-danger">
          {error}
        </p>
      )}

      {(ticket.branchName || latestRunWorktreePath) && (
        <dl className="space-y-2 rounded-md border border-border bg-surface px-3 py-3">
          {ticket.branchName && (
            <div className="space-y-1">
              <dt className="font-body text-xs font-medium text-text-muted">Branch</dt>
              <dd
                className="truncate font-mono text-xs text-text-primary"
                title={ticket.branchName}
              >
                {ticket.branchName}
              </dd>
            </div>
          )}
          {latestRunWorktreePath && (
            <div className="space-y-1">
              <dt className="font-body text-xs font-medium text-text-muted">Worktree</dt>
              <dd
                className="truncate font-mono text-xs text-text-primary"
                title={latestRunWorktreePath}
              >
                {latestRunWorktreePath}
              </dd>
            </div>
          )}
        </dl>
      )}

      <TicketGitActions ticket={ticket} />

      <div className="space-y-2">
        {ticket.humanReview && (
          <ReviewedCommit review={ticket.humanReview} />
        )}
        <Label htmlFor="ticket-repo">Repository</Label>
        <Combobox
          id="ticket-repo"
          value={repoId}
          onValueChange={setRepoId}
          disabled={formDisabled}
          placeholder="None"
          searchPlaceholder="Search repositories…"
          clearable
          options={(repos ?? []).map((repo) => ({
            value: repo.id,
            label:
              repo.verificationStatus !== 'ready'
                ? `${repo.name} (${repo.verificationStatus.replaceAll('_', ' ')})`
                : repo.name,
          }))}
        />
        <Button
          type="button"
          variant="secondary"
          disabled={!ticket.repoId || repoNotReady}
          onClick={() => {
            const url = buildCodeReviewUrl(
              ticket,
              ticket.repoId!,
              worktreePath,
            );
            window.open(url, '_blank', 'noopener,noreferrer');
          }}
          className="w-full"
        >
          Review code
        </Button>
      </div>

      <div className="space-y-2">
        <Label htmlFor="ticket-status">Status</Label>
        <Combobox
          id="ticket-status"
          value={status}
          onValueChange={(value) => setStatus(value as TicketStatus)}
          disabled={formDisabled}
          triggerClassName="h-auto min-h-10 py-2"
          searchPlaceholder="Search statuses…"
          options={BOARD_COLUMNS.map((column) => ({
            value: column.status,
            label: column.label,
          }))}
          renderLabel={(option) => (
            <TicketStatusBadge status={option.value as TicketStatus} />
          )}
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="ticket-priority">Priority</Label>
        <Combobox
          id="ticket-priority"
          value={priority}
          onValueChange={setPriority}
          disabled={formDisabled}
          placeholder="None"
          clearable
          options={ticketPrioritySchema.options.map((p) => ({
            value: p,
            label: p.charAt(0).toUpperCase() + p.slice(1),
          }))}
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="ticket-substatus">Substatus</Label>
        <Combobox
          id="ticket-substatus"
          value={substatus}
          onValueChange={(value) => {
            const next = value as Substatus | '';
            setSubstatus(next);
            if (!next) setMetadata({});
          }}
          disabled={formDisabled}
          placeholder="None"
          searchPlaceholder="Search substatuses…"
          clearable
          options={SUBSTATUSES.map((value) => ({
            value,
            label: SUBSTATUS_LABELS[value],
          }))}
        />
      </div>

      {activeSubstatus === 'waiting_for_agent' && (
        <div className="space-y-2">
          <Label htmlFor="substatus-agent">Agent</Label>
          <Combobox
            id="substatus-agent"
            value={String(metadata.agentId ?? '')}
            onValueChange={(value) => updateMetadataField('agentId', value)}
            disabled={formDisabled}
            placeholder="Select agent…"
            searchPlaceholder="Search agents…"
            clearable
            options={(agents ?? [])
              .filter((agent) => agent.enabled)
              .map((agent) => ({
                value: agent.id,
                label: `${agent.name} (${agent.role})`,
              }))}
          />
        </div>
      )}

      {activeSubstatus === 'blocked_by_missing_capability' && (
        <div className="space-y-2">
          <Label htmlFor="substatus-capability">Capability</Label>
          <Input
            id="substatus-capability"
            value={String(metadata.capability ?? '')}
            onChange={(e) => updateMetadataField('capability', e.target.value)}
          />
        </div>
      )}

      {activeSubstatus === 'blocked_by_missing_secret' && (
        <div className="space-y-2">
          <Label htmlFor="substatus-secret">Secret key</Label>
          <Input
            id="substatus-secret"
            value={String(metadata.secretKey ?? '')}
            onChange={(e) => updateMetadataField('secretKey', e.target.value)}
          />
        </div>
      )}

      {activeSubstatus && substatusOptionalReason(activeSubstatus) && (
        <div className="space-y-2">
          <Label htmlFor="substatus-reason">Reason (optional)</Label>
          <Input
            id="substatus-reason"
            value={String(metadata.reason ?? '')}
            onChange={(e) => updateMetadataField('reason', e.target.value)}
          />
        </div>
      )}

      {activeSubstatus && substatusRequiresMetadata(activeSubstatus) && (
        <p className="font-body text-xs text-text-muted">
          {SUBSTATUS_LABELS[activeSubstatus]} requires additional metadata before
          saving.
        </p>
      )}

      <Button
        type="button"
        onClick={() => void handleSave()}
        loading={isBusy}
        disabled={formDisabled}
        className="w-full"
      >
        {isBusy ? 'Saving…' : 'Save metadata'}
      </Button>
    </div>
  );
}
