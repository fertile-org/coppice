import { useEffect, useState, type FormEvent, type ReactNode } from 'react';
import { useNavigate } from 'react-router-dom';
import { Button } from '../../components/ui/button';
import { Input } from '../../components/ui/input';
import { Label } from '../../components/ui/label';
import { Textarea } from '../../components/ui/textarea';
import { parseApiErrorMessage } from '../../lib/api';
import type { ChatSession } from '../../lib/schemas/chat';
import {
  knowledgeScopeSchema,
  knowledgeTypeSchema,
  type KnowledgeScope,
  type KnowledgeType,
} from '../../lib/schemas/knowledge';
import { useBoards } from '../boards/useBoards';
import { useOpenTicket } from '../tickets/useOpenTicket';
import { ThinkingIndicator } from './ChatMessageList';
import {
  useCreateKnowledgeFromChat,
  useCreateTicketFromChat,
  useCutoffChatSession,
  useDraftTicketFromChat,
} from './useChat';

const KNOWLEDGE_TYPES = knowledgeTypeSchema.options;
const KNOWLEDGE_SCOPES = knowledgeScopeSchema.options;

const TYPE_LABELS: Record<KnowledgeType, string> = {
  coding_convention: 'Coding convention',
  architecture_rule: 'Architecture rule',
  bug_pattern: 'Bug pattern',
  test_command: 'Test command',
  review_feedback: 'Review feedback',
  dependency_note: 'Dependency note',
  api_contract: 'API contract',
  workflow_rule: 'Workflow rule',
  human_preference: 'Human preference',
  operational_runbook: 'Operational runbook',
  security_rule: 'Security rule',
  performance_note: 'Performance note',
};

type DialogKind = 'ticket' | 'knowledge' | null;

function ActionDialogShell({
  title,
  description,
  onClose,
  children,
}: {
  title: string;
  description: string;
  onClose: () => void;
  children: ReactNode;
}) {
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 z-[60] flex items-center justify-center bg-overlay px-4"
      role="presentation"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="chat-action-dialog-title"
        className="w-full max-w-lg overflow-hidden rounded-xl border border-border bg-paper-50 shadow-lg"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="border-b border-border px-5 py-4">
          <h2
            id="chat-action-dialog-title"
            className="font-display text-lg font-semibold text-text-primary"
          >
            {title}
          </h2>
          <p className="mt-1 font-body text-sm text-text-secondary">
            {description}
          </p>
        </div>
        <div className="px-5 py-4">{children}</div>
      </div>
    </div>
  );
}

function CreateTicketDialog({
  session,
  onClose,
}: {
  session: ChatSession;
  onClose: () => void;
}) {
  const { data: boards = [] } = useBoards();
  const createTicket = useCreateTicketFromChat(session.id);
  const {
    data: draft,
    isLoading: draftLoading,
    isError: draftError,
  } = useDraftTicketFromChat(session.id);
  const openTicket = useOpenTicket();
  const [boardId, setBoardId] = useState(
    session.boardId ?? boards[0]?.id ?? '',
  );
  const [title, setTitle] = useState('');
  const [description, setDescription] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [draftApplied, setDraftApplied] = useState(false);

  useEffect(() => {
    if (!boardId && boards[0]?.id) {
      setBoardId(session.boardId ?? boards[0].id);
    }
  }, [boardId, boards, session.boardId]);

  useEffect(() => {
    if (!draft || draftApplied) return;
    setTitle(draft.title);
    setDescription(draft.description);
    setDraftApplied(true);
  }, [draft, draftApplied]);

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    if (!boardId) {
      setError('Select a board.');
      return;
    }
    setError(null);
    try {
      const result = await createTicket.mutateAsync({
        boardId,
        title: title.trim() || undefined,
        description: description.trim() || undefined,
      });
      onClose();
      await openTicket(result.ticket.id);
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Could not create ticket.'));
    }
  }

  const draftHint = draftError
    ? 'Could not load an agent draft; edit the fields below or confirm to use server defaults.'
    : draft?.source === 'fallback'
      ? 'Using a deterministic fallback draft. Edit before confirming.'
      : 'Review the agent draft, edit if needed, then confirm.';

  return (
    <ActionDialogShell
      title="Create ticket"
      description="Confirm a board ticket drafted from this chat. Nothing is created until you confirm."
      onClose={onClose}
    >
      <form
        onSubmit={(event) => void handleSubmit(event)}
        className="space-y-3"
        data-testid="create-ticket-dialog"
      >
        <div
          className="font-body text-sm text-text-secondary"
          data-testid="draft-ticket-status"
          aria-busy={draftLoading || undefined}
        >
          {draftLoading ? (
            <ThinkingIndicator label="Drafting title and description from this chat… This can take up to about a minute." />
          ) : (
            <p>{draftHint}</p>
          )}
        </div>
        <div className="space-y-2">
          <Label htmlFor="chat-ticket-board">Board</Label>
          <select
            id="chat-ticket-board"
            aria-label="Board"
            className="field-control h-10 w-full px-3 font-body text-sm"
            value={boardId}
            onChange={(event) => setBoardId(event.target.value)}
            required
          >
            <option value="">Select board</option>
            {boards.map((board) => (
              <option key={board.id} value={board.id}>
                {board.name}
              </option>
            ))}
          </select>
        </div>
        <div className="space-y-2">
          <Label htmlFor="chat-ticket-title">Title</Label>
          <Input
            id="chat-ticket-title"
            aria-label="Title"
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            placeholder={draftLoading ? 'Drafting…' : 'Board-ready ticket title'}
            disabled={draftLoading}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor="chat-ticket-description">Description</Label>
          <Textarea
            id="chat-ticket-description"
            aria-label="Description"
            value={description}
            onChange={(event) => setDescription(event.target.value)}
            rows={4}
            placeholder={
              draftLoading
                ? 'Drafting…'
                : 'Structured description with context and next steps'
            }
            disabled={draftLoading}
          />
        </div>
        {error && (
          <p className="font-body text-sm text-danger" role="alert">
            {error}
          </p>
        )}
        <div className="flex justify-end gap-2 pt-1">
          <Button type="button" variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button
            type="submit"
            disabled={createTicket.isPending || draftLoading || !boardId}
          >
            {createTicket.isPending ? 'Creating…' : 'Confirm create ticket'}
          </Button>
        </div>
      </form>
    </ActionDialogShell>
  );
}

function CreateKnowledgeDialog({
  session,
  onClose,
}: {
  session: ChatSession;
  onClose: () => void;
}) {
  const navigate = useNavigate();
  const { data: boards = [] } = useBoards();
  const createKnowledge = useCreateKnowledgeFromChat(session.id);
  const [title, setTitle] = useState('');
  const [content, setContent] = useState('');
  const [knowledgeType, setKnowledgeType] = useState<KnowledgeType>(
    'coding_convention',
  );
  const [scope, setScope] = useState<KnowledgeScope>('board');
  const [boardId, setBoardId] = useState(
    session.boardId ?? boards[0]?.id ?? '',
  );
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!boardId && boards[0]?.id) {
      setBoardId(session.boardId ?? boards[0].id);
    }
  }, [boardId, boards, session.boardId]);

  const needsBoard = scope === 'board' || scope === 'agent';

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    if (needsBoard && !boardId) {
      setError('Select a board for this scope.');
      return;
    }
    setError(null);
    try {
      await createKnowledge.mutateAsync({
        title: title.trim() || undefined,
        content: content.trim() || undefined,
        knowledgeType,
        scope,
        boardId: needsBoard ? boardId : undefined,
      });
      onClose();
      navigate('/knowledge');
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Could not propose knowledge.'));
    }
  }

  return (
    <ActionDialogShell
      title="Propose knowledge"
      description="Send a compacted summary to the Knowledge Inbox as pending. Approval stays on the inbox."
      onClose={onClose}
    >
      <form
        onSubmit={(event) => void handleSubmit(event)}
        className="space-y-3"
        data-testid="create-knowledge-dialog"
      >
        <div className="space-y-2">
          <Label htmlFor="chat-knowledge-title">Title (optional)</Label>
          <Input
            id="chat-knowledge-title"
            aria-label="Knowledge title (optional)"
            value={title}
            onChange={(event) => setTitle(event.target.value)}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor="chat-knowledge-content">Content (optional)</Label>
          <Textarea
            id="chat-knowledge-content"
            aria-label="Knowledge content (optional)"
            value={content}
            onChange={(event) => setContent(event.target.value)}
            rows={4}
            placeholder="Defaults to a compacted transcript"
          />
        </div>
        <div className="grid gap-3 sm:grid-cols-2">
          <div className="space-y-2">
            <Label htmlFor="chat-knowledge-type">Type</Label>
            <select
              id="chat-knowledge-type"
              aria-label="Knowledge type"
              className="field-control h-10 w-full px-3 font-body text-sm"
              value={knowledgeType}
              onChange={(event) =>
                setKnowledgeType(event.target.value as KnowledgeType)
              }
            >
              {KNOWLEDGE_TYPES.map((type) => (
                <option key={type} value={type}>
                  {TYPE_LABELS[type]}
                </option>
              ))}
            </select>
          </div>
          <div className="space-y-2">
            <Label htmlFor="chat-knowledge-scope">Scope</Label>
            <select
              id="chat-knowledge-scope"
              aria-label="Knowledge scope"
              className="field-control h-10 w-full px-3 font-body text-sm"
              value={scope}
              onChange={(event) =>
                setScope(event.target.value as KnowledgeScope)
              }
            >
              {KNOWLEDGE_SCOPES.map((value) => (
                <option key={value} value={value}>
                  {value}
                </option>
              ))}
            </select>
          </div>
        </div>
        {needsBoard && (
          <div className="space-y-2">
            <Label htmlFor="chat-knowledge-board">Board</Label>
            <select
              id="chat-knowledge-board"
              aria-label="Knowledge board"
              className="field-control h-10 w-full px-3 font-body text-sm"
              value={boardId}
              onChange={(event) => setBoardId(event.target.value)}
              required
            >
              <option value="">Select board</option>
              {boards.map((board) => (
                <option key={board.id} value={board.id}>
                  {board.name}
                </option>
              ))}
            </select>
          </div>
        )}
        {error && (
          <p className="font-body text-sm text-danger" role="alert">
            {error}
          </p>
        )}
        <div className="flex justify-end gap-2 pt-1">
          <Button type="button" variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button
            type="submit"
            disabled={
              createKnowledge.isPending || (needsBoard && !boardId)
            }
          >
            {createKnowledge.isPending
              ? 'Proposing…'
              : 'Confirm propose knowledge'}
          </Button>
        </div>
      </form>
    </ActionDialogShell>
  );
}

export function ChatSessionActions({
  session,
  disabled,
}: {
  session: ChatSession;
  disabled?: boolean;
}) {
  const navigate = useNavigate();
  const cutoff = useCutoffChatSession(session.id);
  const [dialog, setDialog] = useState<DialogKind>(null);
  const [error, setError] = useState<string | null>(null);
  const inactive =
    session.status === 'cutoff' || session.status === 'archived';

  async function handleCutoff() {
    const confirmed = window.confirm(
      'Cutoff this session? A shorter child chat will open with a summary seed. This session stays readable.',
    );
    if (!confirmed) return;
    setError(null);
    try {
      const result = await cutoff.mutateAsync();
      navigate(`/chat/${result.child.id}`);
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Could not cutoff session.'));
    }
  }

  if (inactive) return null;

  return (
    <div className="flex flex-col items-end gap-1">
      <div
        className="flex flex-wrap justify-end gap-2"
        role="group"
        aria-label="Chat session actions"
        data-testid="chat-session-actions"
      >
        <Button
          type="button"
          variant="secondary"
          size="sm"
          disabled={disabled || cutoff.isPending}
          onClick={() => setDialog('ticket')}
        >
          Create ticket
        </Button>
        <Button
          type="button"
          variant="secondary"
          size="sm"
          disabled={disabled || cutoff.isPending}
          onClick={() => setDialog('knowledge')}
        >
          Propose knowledge
        </Button>
        <Button
          type="button"
          variant="secondary"
          size="sm"
          disabled={disabled || cutoff.isPending}
          onClick={() => void handleCutoff()}
        >
          {cutoff.isPending ? 'Cutting off…' : 'Cutoff'}
        </Button>
      </div>
      {error && (
        <p className="font-body text-xs text-danger" role="alert">
          {error}
        </p>
      )}
      {dialog === 'ticket' && (
        <CreateTicketDialog
          session={session}
          onClose={() => setDialog(null)}
        />
      )}
      {dialog === 'knowledge' && (
        <CreateKnowledgeDialog
          session={session}
          onClose={() => setDialog(null)}
        />
      )}
    </div>
  );
}
