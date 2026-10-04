import { useEffect, useMemo, useState, type FormEvent } from 'react';
import { useNavigate } from 'react-router-dom';
import { useToast } from '../../components/ToastProvider';
import { Button } from '../../components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '../../components/ui/dialog';
import { Input } from '../../components/ui/input';
import { Label } from '../../components/ui/label';
import { Combobox } from '../../components/ui/combobox';
import { Textarea } from '../../components/ui/textarea';
import type { InlineComment } from '../../lib/schemas/codeReview';
import { apiFetch } from '../../lib/api';
import { useAgents } from '../agents/useAgents';
import { useBoards } from '../boards/useBoards';
import { useTicket } from '../tickets/useTicket';
import { formatReviewPreview } from './formatReviewPreview';
import { useSubmitCodeReview } from './useCodeReview';

type WorkflowAction = 'none' | 'move_to_in_progress' | 'reassign_engineer';

interface SubmitReviewDialogProps {
  open: boolean;
  onClose: () => void;
  repoId: string;
  repoName: string;
  worktreePath: string;
  baseBranch: string;
  headBranch: string;
  headSha: string;
  ticketId: string | undefined;
  inlineComments: InlineComment[];
  onSubmitted: () => void;
}

function engineerAgents(agents: ReturnType<typeof useAgents>['data']) {
  return (agents ?? []).filter(
    (agent) => agent.enabled && agent.role.toLowerCase().includes('engineer'),
  );
}

export function SubmitReviewDialog({
  open,
  onClose,
  repoId,
  repoName,
  worktreePath,
  baseBranch,
  headBranch,
  headSha,
  ticketId,
  inlineComments,
  onSubmitted,
}: SubmitReviewDialogProps) {
  const toast = useToast();
  const navigate = useNavigate();
  const submitReview = useSubmitCodeReview();
  const { data: boards } = useBoards();
  const { data: ticket } = useTicket(ticketId);
  const { data: agents } = useAgents();

  const [summary, setSummary] = useState('');
  const [boardId, setBoardId] = useState('');
  const [title, setTitle] = useState('');
  const [description, setDescription] = useState('');
  const [workflowAction, setWorkflowAction] =
    useState<WorkflowAction>('none');
  const [reassignAgentId, setReassignAgentId] = useState('');
  const [error, setError] = useState<string | null>(null);

  const engineers = useMemo(() => engineerAgents(agents), [agents]);

  useEffect(() => {
    if (!open) return;
    setSummary('');
    setBoardId(boards?.[0]?.id ?? '');
    setTitle('');
    setDescription('');
    setWorkflowAction('none');
    setReassignAgentId(ticket?.assigneeAgentId ?? engineers[0]?.id ?? '');
    setError(null);
  }, [open, boards, ticket?.assigneeAgentId, engineers]);

  const preview = useMemo(
    () =>
      formatReviewPreview(
        repoName,
        worktreePath,
        baseBranch,
        headBranch,
        headSha,
        summary.trim() || '(summary required)',
        inlineComments,
      ),
    [
      repoName,
      worktreePath,
      baseBranch,
      headBranch,
      headSha,
      summary,
      inlineComments,
    ],
  );

  async function resolveBoardId(resultTicketId: string): Promise<string | null> {
    if (ticket?.boardId) return ticket.boardId;
    try {
      const res = await apiFetch(`/api/tickets/${resultTicketId}`);
      const data = (await res.json()) as { boardId: string };
      return data.boardId;
    } catch {
      return null;
    }
  }

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    if (!summary.trim()) {
      setError('Review summary is required.');
      return;
    }

    if (!ticketId) {
      if (!boardId) {
        setError('Select a board.');
        return;
      }
      if (!title.trim()) {
        setError('Ticket title is required.');
        return;
      }
    }

    if (workflowAction === 'reassign_engineer' && !reassignAgentId) {
      setError('Select an engineer to reassign.');
      return;
    }

    setError(null);

    try {
      const result = await submitReview.mutateAsync({
        repoId,
        worktreePath,
        baseBranch,
        headSha,
        ticketId: ticketId ?? null,
        newTicket: ticketId
          ? undefined
          : {
              boardId,
              title: title.trim(),
              description: description.trim() || undefined,
            },
        summary: summary.trim(),
        inlineComments,
        workflowAction: ticketId ? workflowAction : undefined,
        reassignAgentId:
          ticketId && workflowAction === 'reassign_engineer'
            ? reassignAgentId
            : undefined,
      });

      onSubmitted();
      onClose();

      const resolvedBoardId = await resolveBoardId(result.ticketId);
      toast.success('Review posted');
      if (resolvedBoardId) {
        navigate(
          `/boards/${resolvedBoardId}?ticket=${result.ticketId}`,
        );
      }
    } catch {
      setError('Unable to submit review. Refresh the diff and try again.');
      toast.error('Unable to submit review');
    }
  }

  return (
    <Dialog open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogContent
        className="z-[60] flex max-h-[90vh] max-w-2xl flex-col overflow-hidden p-0"
        overlayClassName="z-[60]"
      >
        <div className="border-b border-border px-6 py-4">
          <DialogTitle>Submit review</DialogTitle>
          <DialogDescription className="mt-1">
            Post a combined review comment
            {ticketId ? ' on the linked ticket' : ' as a new ticket'}.
          </DialogDescription>
        </div>

        <form
          onSubmit={(e) => void handleSubmit(e)}
          className="flex min-h-0 flex-1 flex-col overflow-hidden"
        >
          <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-6 py-4">
            <div className="space-y-2">
              <Label htmlFor="review-summary">Summary</Label>
              <Textarea
                id="review-summary"
                value={summary}
                onChange={(e) => setSummary(e.target.value)}
                placeholder="Overall feedback for this change…"
                rows={4}
                required
              />
            </div>

            {!ticketId && (
              <>
                <div className="space-y-2">
                  <Label htmlFor="review-board">Board</Label>
                  <Combobox
                    id="review-board"
                    value={boardId}
                    onValueChange={setBoardId}
                    placeholder="Select board…"
                    searchPlaceholder="Search boards…"
                    options={(boards ?? []).map((board) => ({
                      value: board.id,
                      label: board.name,
                    }))}
                  />
                </div>

                <div className="space-y-2">
                  <Label htmlFor="review-title">Ticket title</Label>
                  <Input
                    id="review-title"
                    value={title}
                    onChange={(e) => setTitle(e.target.value)}
                    placeholder="What needs follow-up?"
                    required
                  />
                </div>

                <div className="space-y-2">
                  <Label htmlFor="review-description">
                    Description{' '}
                    <span className="text-text-muted">(optional)</span>
                  </Label>
                  <Textarea
                    id="review-description"
                    value={description}
                    onChange={(e) => setDescription(e.target.value)}
                    rows={3}
                  />
                </div>
              </>
            )}

            {ticketId && (
              <div className="space-y-2">
                <Label htmlFor="review-workflow">Workflow action</Label>
                <Combobox
                  id="review-workflow"
                  value={workflowAction}
                  onValueChange={(value) =>
                    setWorkflowAction(value as WorkflowAction)
                  }
                  options={[
                    { value: 'none', label: 'Comment only' },
                    { value: 'move_to_in_progress', label: 'Move to In Progress' },
                    { value: 'reassign_engineer', label: 'Reassign engineer' },
                  ]}
                />
              </div>
            )}

            {ticketId && workflowAction === 'reassign_engineer' && (
              <div className="space-y-2">
                <Label htmlFor="review-engineer">Engineer</Label>
                <Combobox
                  id="review-engineer"
                  value={reassignAgentId}
                  onValueChange={setReassignAgentId}
                  placeholder="Select engineer…"
                  searchPlaceholder="Search engineers…"
                  options={engineers.map((agent) => ({
                    value: agent.id,
                    label: agent.role ? `${agent.name} · ${agent.role}` : agent.name,
                  }))}
                />
              </div>
            )}

            <div className="space-y-2">
              <Label>Preview</Label>
              <pre className="max-h-48 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-surface px-3 py-2 font-mono text-xs text-text-secondary">
                {preview}
              </pre>
            </div>
          </div>

          {error && (
            <p className="px-6 pb-2 font-body text-sm text-danger" role="alert">
              {error}
            </p>
          )}

          <div className="flex justify-end gap-2 border-t border-border px-6 py-4">
            <Button type="button" variant="secondary" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" loading={submitReview.isPending}>
              Submit review
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
