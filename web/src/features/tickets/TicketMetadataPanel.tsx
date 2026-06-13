import { useEffect, useState } from 'react';
import { BOARD_COLUMNS, type TicketStatus } from '../board/columns';
import type { Ticket } from '../board/useTickets';
import { useToast } from '../../components/ToastProvider';
import { Button } from '../../components/ui/button';
import { Input } from '../../components/ui/input';
import { Label } from '../../components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../../components/ui/select';
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
  useUpdateTicket,
  useUpdateTicketStatus,
} from './useTicket';

interface TicketMetadataPanelProps {
  ticket: Ticket;
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

  async function handleAssignChange(nextAgentId: string) {
    setAssignError(null);
    setAssigneeId(nextAgentId);
    try {
      await assignAgent.mutateAsync(nextAgentId || null);
    } catch {
      setAssigneeId(ticket.assigneeAgentId ?? '');
      setAssignError('Unable to update assignee.');
    }
  }

  function updateMetadataField(key: string, value: string) {
    setMetadata((prev) => ({ ...prev, [key]: value }));
  }

  async function handleSave() {
    setError(null);

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

    setIsSaving(true);
    try {
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
      setError('Unable to save metadata.');
      toast.error('Unable to save metadata');
    } finally {
      setIsSaving(false);
    }
  }

  const isBusy =
    isSaving ||
    updateStatus.isPending ||
    updateTicket.isPending ||
    approveSplits.isPending ||
    dismissSplits.isPending;
  const activeSubstatus = substatus || null;
  const assignedAgent = agents?.find((agent) => agent.id === assigneeId);

  return (
    <div className="space-y-5">
      <div className="space-y-2">
        <Label htmlFor="ticket-assignee">Assignee</Label>
        <Select
          value={assigneeId || '__none__'}
          onValueChange={(value) =>
            void handleAssignChange(value === '__none__' ? '' : value)
          }
          disabled={assignAgent.isPending}
        >
          <SelectTrigger id="ticket-assignee">
            <SelectValue placeholder="Unassigned" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__none__">Unassigned</SelectItem>
            {agents
              ?.filter((agent) => agent.enabled)
              .map((agent) => (
                <SelectItem key={agent.id} value={agent.id} textValue={agent.name}>
                  {agent.name} ({agent.role})
                </SelectItem>
              ))}
          </SelectContent>
        </Select>
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
              disabled={isBusy}
              className="flex-1"
            >
              {approveSplits.isPending ? 'Approving…' : 'Approve splits'}
            </Button>
            <Button
              type="button"
              variant="secondary"
              onClick={() => void handleDismissSplits()}
              loading={dismissSplits.isPending}
              disabled={isBusy}
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
        <Label htmlFor="ticket-repo">Repository</Label>
        <Select value={repoId || '__none__'} onValueChange={(v) => setRepoId(v === '__none__' ? '' : v)}>
          <SelectTrigger id="ticket-repo">
            <SelectValue placeholder="None" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__none__">None</SelectItem>
            {repos?.map((repo) => (
              <SelectItem
                key={repo.id}
                value={repo.id}
                textValue={repo.name}
              >
                {repo.name}
                {repo.verificationStatus !== 'ready'
                  ? ` (${repo.verificationStatus.replaceAll('_', ' ')})`
                  : ''}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      <div className="space-y-2">
        <Label htmlFor="ticket-status">Status</Label>
        <Select
          value={status}
          onValueChange={(value) => setStatus(value as TicketStatus)}
        >
          <SelectTrigger id="ticket-status" className="h-auto min-h-10 py-2">
            <SelectValue>
              <TicketStatusBadge status={status} />
            </SelectValue>
          </SelectTrigger>
          <SelectContent>
            {BOARD_COLUMNS.map((column) => (
              <SelectItem
                key={column.status}
                value={column.status}
                textValue={column.label}
              >
                <TicketStatusBadge status={column.status} />
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      <div className="space-y-2">
        <Label htmlFor="ticket-priority">Priority</Label>
        <Select
          value={priority || '__none__'}
          onValueChange={(v) => setPriority(v === '__none__' ? '' : v)}
        >
          <SelectTrigger id="ticket-priority">
            <SelectValue placeholder="None" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__none__">None</SelectItem>
            {ticketPrioritySchema.options.map((p) => (
              <SelectItem key={p} value={p} className="capitalize">
                {p}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      <div className="space-y-2">
        <Label htmlFor="ticket-substatus">Substatus</Label>
        <Select
          value={substatus || '__none__'}
          onValueChange={(value) => {
            const next = value === '__none__' ? '' : (value as Substatus);
            setSubstatus(next);
            if (!next) setMetadata({});
          }}
        >
          <SelectTrigger id="ticket-substatus">
            <SelectValue placeholder="None" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__none__">None</SelectItem>
            {SUBSTATUSES.map((value) => (
              <SelectItem key={value} value={value} textValue={SUBSTATUS_LABELS[value]}>
                {SUBSTATUS_LABELS[value]}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {activeSubstatus === 'waiting_for_agent' && (
        <div className="space-y-2">
          <Label htmlFor="substatus-agent">Agent</Label>
          <Select
            value={String(metadata.agentId ?? '__none__')}
            onValueChange={(value) =>
              updateMetadataField('agentId', value === '__none__' ? '' : value)
            }
          >
            <SelectTrigger id="substatus-agent">
              <SelectValue placeholder="Select agent…" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="__none__">Select agent…</SelectItem>
              {agents
                ?.filter((agent) => agent.enabled)
                .map((agent) => (
                  <SelectItem key={agent.id} value={agent.id} textValue={agent.name}>
                    {agent.name} ({agent.role})
                  </SelectItem>
                ))}
            </SelectContent>
          </Select>
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
        disabled={isBusy}
        className="w-full"
      >
        {isBusy ? 'Saving…' : 'Save metadata'}
      </Button>
    </div>
  );
}
