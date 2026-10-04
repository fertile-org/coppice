import { useEffect, useState, type FormEvent, type ReactNode } from 'react';
import { useNavigate } from 'react-router-dom';
import { Button } from '../../components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '../../components/ui/dialog';
import { Combobox } from '../../components/ui/combobox';
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

/** Children mount only while open, so their queries stop after the exit animation. */
function ActionDialogShell({
  open,
  title,
  description,
  onClose,
  children,
}: {
  open: boolean;
  title: string;
  description: string;
  onClose: () => void;
  children: ReactNode;
}) {
  return (
    <Dialog open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogContent
        className="z-[60] max-w-lg overflow-hidden p-0"
        overlayClassName="z-[60]"
      >
        <div className="border-b border-border px-5 py-4">
          <DialogTitle className="text-lg text-text-primary">{title}</DialogTitle>
          <DialogDescription className="mt-1">{description}</DialogDescription>
        </div>
        <div className="px-5 py-4">{children}</div>
      </DialogContent>
    </Dialog>
  );
}

function CreateTicketForm({
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
        <Combobox
          id="chat-ticket-board"
          aria-label="Board"
          value={boardId}
          onValueChange={setBoardId}
          placeholder="Select board"
          searchPlaceholder="Search boards…"
          options={boards.map((board) => ({ value: board.id, label: board.name }))}
        />
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
  );
}

function CreateKnowledgeForm({
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
          <Combobox
            id="chat-knowledge-type"
            aria-label="Knowledge type"
            value={knowledgeType}
            onValueChange={(value) => setKnowledgeType(value as KnowledgeType)}
            searchPlaceholder="Search types…"
            options={KNOWLEDGE_TYPES.map((type) => ({
              value: type,
              label: TYPE_LABELS[type],
            }))}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor="chat-knowledge-scope">Scope</Label>
          <Combobox
            id="chat-knowledge-scope"
            aria-label="Knowledge scope"
            value={scope}
            onValueChange={(value) => setScope(value as KnowledgeScope)}
            options={KNOWLEDGE_SCOPES.map((value) => ({ value, label: value }))}
          />
        </div>
      </div>
      {needsBoard && (
        <div className="space-y-2">
          <Label htmlFor="chat-knowledge-board">Board</Label>
          <Combobox
            id="chat-knowledge-board"
            aria-label="Knowledge board"
            value={boardId}
            onValueChange={setBoardId}
            placeholder="Select board"
            searchPlaceholder="Search boards…"
            options={boards.map((board) => ({ value: board.id, label: board.name }))}
          />
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
      <ActionDialogShell
        open={dialog === 'ticket'}
        title="Create ticket"
        description="Confirm a board ticket drafted from this chat. Nothing is created until you confirm."
        onClose={() => setDialog(null)}
      >
        <CreateTicketForm session={session} onClose={() => setDialog(null)} />
      </ActionDialogShell>
      <ActionDialogShell
        open={dialog === 'knowledge'}
        title="Propose knowledge"
        description="Send a compacted summary to the Knowledge Inbox as pending. Approval stays on the inbox."
        onClose={() => setDialog(null)}
      >
        <CreateKnowledgeForm session={session} onClose={() => setDialog(null)} />
      </ActionDialogShell>
    </div>
  );
}
