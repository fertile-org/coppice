import { useEffect, useMemo, useState, type FormEvent } from 'react';
import { useNavigate } from 'react-router-dom';
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
import { Textarea } from '../../components/ui/textarea';
import type { InlineComment } from '../../lib/schemas/codeReview';
import { apiFetch } from '../../lib/api';
import { useAgents } from '../agents/useAgents';
import { useProjects } from '../projects/useProjects';
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
  const { data: projects } = useProjects();
  const { data: ticket } = useTicket(ticketId);
  const { data: agents } = useAgents();

  const [summary, setSummary] = useState('');
  const [projectId, setProjectId] = useState('');
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
    setProjectId(projects?.[0]?.id ?? '');
    setTitle('');
    setDescription('');
    setWorkflowAction('none');
    setReassignAgentId(ticket?.assigneeAgentId ?? engineers[0]?.id ?? '');
    setError(null);
  }, [open, projects, ticket?.assigneeAgentId, engineers]);

  useEffect(() => {
    if (!open) return;
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [open, onClose]);

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

  if (!open) return null;

  async function resolveProjectId(resultTicketId: string): Promise<string | null> {
    if (ticket?.projectId) return ticket.projectId;
    try {
      const res = await apiFetch(`/api/tickets/${resultTicketId}`);
      const data = (await res.json()) as { projectId: string };
      return data.projectId;
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
      if (!projectId) {
        setError('Select a project.');
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
              projectId,
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

      const resolvedProjectId = await resolveProjectId(result.ticketId);
      toast.success('Review posted');
      if (resolvedProjectId) {
        navigate(
          `/projects/${resolvedProjectId}/board?ticket=${result.ticketId}`,
        );
      }
    } catch {
      setError('Unable to submit review. Refresh the diff and try again.');
      toast.error('Unable to submit review');
    }
  }

  return (
    <div
      className="fixed inset-0 z-[60] flex items-center justify-center bg-bark-950/40 px-4"
      role="presentation"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="submit-review-title"
        className="flex max-h-[90vh] w-full max-w-2xl flex-col overflow-hidden rounded-xl border border-border bg-paper-50 shadow-lg"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="border-b border-border px-6 py-4">
          <h2
            id="submit-review-title"
            className="font-display text-xl font-semibold text-bark-900"
          >
            Submit review
          </h2>
          <p className="mt-1 font-body text-sm text-text-secondary">
            Post a combined review comment
            {ticketId ? ' on the linked ticket' : ' as a new ticket'}.
          </p>
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
                  <Label htmlFor="review-project">Project</Label>
                  <Select value={projectId} onValueChange={setProjectId}>
                    <SelectTrigger id="review-project">
                      <SelectValue placeholder="Select project…" />
                    </SelectTrigger>
                    <SelectContent>
                      {(projects ?? []).map((project) => (
                        <SelectItem
                          key={project.id}
                          value={project.id}
                          textValue={project.name}
                        >
                          {project.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
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
                <Select
                  value={workflowAction}
                  onValueChange={(value) =>
                    setWorkflowAction(value as WorkflowAction)
                  }
                >
                  <SelectTrigger id="review-workflow">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="none" textValue="Comment only">
                      Comment only
                    </SelectItem>
                    <SelectItem
                      value="move_to_in_progress"
                      textValue="Move to In Progress"
                    >
                      Move to In Progress
                    </SelectItem>
                    <SelectItem
                      value="reassign_engineer"
                      textValue="Reassign engineer"
                    >
                      Reassign engineer
                    </SelectItem>
                  </SelectContent>
                </Select>
              </div>
            )}

            {ticketId && workflowAction === 'reassign_engineer' && (
              <div className="space-y-2">
                <Label htmlFor="review-engineer">Engineer</Label>
                <Select
                  value={reassignAgentId}
                  onValueChange={setReassignAgentId}
                >
                  <SelectTrigger id="review-engineer">
                    <SelectValue placeholder="Select engineer…" />
                  </SelectTrigger>
                  <SelectContent>
                    {engineers.map((agent) => (
                      <SelectItem
                        key={agent.id}
                        value={agent.id}
                        textValue={agent.name}
                      >
                        {agent.name}
                        {agent.role ? ` · ${agent.role}` : ''}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
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
      </div>
    </div>
  );
}
