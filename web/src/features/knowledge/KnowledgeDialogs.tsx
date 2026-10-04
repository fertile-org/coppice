import { useMemo, useState, type FormEvent } from 'react';
import { useToast } from '../../components/ToastProvider';
import { Button } from '../../components/ui/button';
import { Combobox } from '../../components/ui/combobox';
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../../components/ui/dialog';
import { Input } from '../../components/ui/input';
import { Label } from '../../components/ui/label';
import { Textarea } from '../../components/ui/textarea';
import { parseApiErrorMessage } from '../../lib/api';
import type {
  KnowledgeConfidence,
  KnowledgeItem,
  KnowledgeScope,
  KnowledgeType,
} from '../../lib/schemas/knowledge';
import { useAgents } from '../agents/useAgents';
import { useBoards } from '../boards/useBoards';
import { REJECT_PRESETS } from './curationGuide';
import {
  CONFIDENCE_OPTIONS,
  TYPE_LABELS,
  TYPE_OPTIONS,
  scopeLabel,
} from './knowledgeFormat';
import {
  useCreateKnowledge,
  useEditKnowledge,
  useRejectKnowledge,
  useSupersedeKnowledge,
  type KnowledgeRevisionInput,
} from './useKnowledge';

export type KnowledgeFormMode = 'create' | 'edit' | 'supersede';

interface CandidateFormState {
  scope: KnowledgeScope;
  boardId: string;
  agentId: string;
  knowledgeType: KnowledgeType;
  title: string;
  content: string;
  confidence: KnowledgeConfidence;
}

const EMPTY_CANDIDATE: CandidateFormState = {
  scope: 'board',
  boardId: '',
  agentId: '',
  knowledgeType: 'coding_convention',
  title: '',
  content: '',
  confidence: 'medium',
};

const SCOPE_OPTIONS = [
  { value: 'workspace', label: 'Workspace', description: 'Every board and agent' },
  { value: 'board', label: 'Board', description: 'Runs on one board' },
  { value: 'agent', label: 'Board + agent', description: 'One agent on one board' },
];

const MODE_COPY: Record<
  KnowledgeFormMode,
  { title: string; description: string; submit: string; success: string }
> = {
  create: {
    title: 'Add knowledge',
    description: 'New notes begin in Pending so their scope and wording can be reviewed.',
    submit: 'Add to Pending',
    success: 'Knowledge added to Pending.',
  },
  edit: {
    title: 'Create a new revision',
    description: 'The previous revision remains in the audit history.',
    submit: 'Save revision',
    success: 'Revision saved.',
  },
  supersede: {
    title: 'Create replacement candidate',
    description: 'The current item stays usable until the replacement is approved.',
    submit: 'Create replacement',
    success: 'Replacement candidate added to Pending.',
  },
};

function candidateInput(form: CandidateFormState): KnowledgeRevisionInput {
  return {
    scope: form.scope,
    boardId: form.scope === 'workspace' ? null : form.boardId,
    agentId: form.scope === 'agent' ? form.agentId : null,
    knowledgeType: form.knowledgeType,
    title: form.title.trim(),
    content: form.content.trim(),
    sourceType: 'human_note',
    sourceId: null,
    sourceRunId: null,
    confidence: form.confidence,
  };
}

function initialForm(item: KnowledgeItem | null): CandidateFormState {
  if (!item) return EMPTY_CANDIDATE;
  return {
    scope: item.scope,
    boardId: item.boardId ?? '',
    agentId: item.agentId ?? '',
    knowledgeType: item.knowledgeType,
    title: item.title,
    content: item.content,
    confidence: item.confidence,
  };
}

function FormError({ message }: { message: string | null }) {
  if (!message) return null;
  return (
    <p
      role="alert"
      className="rounded-md bg-danger-muted px-3 py-2 font-body text-sm text-danger"
    >
      {message}
    </p>
  );
}

function KnowledgeForm({
  mode,
  item,
  onDone,
}: {
  mode: KnowledgeFormMode;
  item: KnowledgeItem | null;
  onDone: () => void;
}) {
  const toast = useToast();
  const { data: boards } = useBoards();
  const { data: agents } = useAgents();
  const create = useCreateKnowledge();
  const edit = useEditKnowledge();
  const supersede = useSupersedeKnowledge();
  const [form, setForm] = useState<CandidateFormState>(() => initialForm(item));
  const [error, setError] = useState<string | null>(null);
  const copy = MODE_COPY[mode];
  const isCreate = mode === 'create';

  const boardOptions = useMemo(
    () => boards?.map((board) => ({ value: board.id, label: board.name })) ?? [],
    [boards],
  );
  const agentOptions = useMemo(
    () =>
      agents
        ?.filter((agent) => agent.enabled)
        .map((agent) => ({ value: agent.id, label: agent.name })) ?? [],
    [agents],
  );

  function update(patch: Partial<CandidateFormState>) {
    setForm((value) => ({ ...value, ...patch }));
  }

  async function save(): Promise<void> {
    if (mode === 'create') {
      await create.mutateAsync(candidateInput(form));
      return;
    }
    if (!item) return;
    if (mode === 'edit') {
      await edit.mutateAsync({
        id: item.id,
        expectedVersion: item.version,
        patch: {
          title: form.title.trim(),
          content: form.content.trim(),
          confidence: form.confidence,
        },
      });
      return;
    }
    await supersede.mutateAsync({
      id: item.id,
      expectedVersion: item.version,
      replacement: {
        scope: item.scope,
        boardId: item.boardId,
        agentId: item.agentId,
        knowledgeType: item.knowledgeType,
        title: form.title.trim(),
        content: form.content.trim(),
        sourceType: 'human_note',
        sourceId: null,
        sourceRunId: null,
        confidence: form.confidence,
      },
    });
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (isCreate && form.scope !== 'workspace' && !form.boardId) {
      setError('Choose a board for this scope.');
      return;
    }
    if (isCreate && form.scope === 'agent' && !form.agentId) {
      setError('Choose an agent for agent-scoped knowledge.');
      return;
    }
    if (!form.title.trim() || !form.content.trim()) {
      setError('Title and content are required.');
      return;
    }
    setError(null);
    try {
      await save();
      toast.success(copy.success);
      onDone();
    } catch (cause) {
      setError(
        parseApiErrorMessage(
          cause,
          isCreate
            ? 'Unable to create candidate.'
            : 'Unable to update this knowledge item. Refresh and try again.',
        ),
      );
    }
  }

  const pending = create.isPending || edit.isPending || supersede.isPending;

  return (
    <form onSubmit={(event) => void submit(event)} noValidate>
      <DialogHeader>
        <DialogTitle>{copy.title}</DialogTitle>
        <DialogDescription>{copy.description}</DialogDescription>
      </DialogHeader>

      {item && !isCreate && (
        <p className="mt-4 rounded-md border border-border bg-paper-100 px-3 py-2 font-body text-xs text-text-secondary">
          <span className="font-medium text-text-primary">
            {TYPE_LABELS[item.knowledgeType]}
          </span>{' '}
          · {scopeLabel(item)} · revision {item.revisionNumber}
        </p>
      )}

      <div className="mt-5 space-y-4">
        <div className="space-y-1.5">
          <Label htmlFor="knowledge-title">Title</Label>
          <Input
            id="knowledge-title"
            required
            maxLength={160}
            value={form.title}
            onChange={(event) => update({ title: event.target.value })}
            placeholder="Run unit tests before review"
          />
        </div>

        <div className="space-y-1.5">
          <Label htmlFor="knowledge-content">Knowledge</Label>
          <Textarea
            id="knowledge-content"
            required
            maxLength={12000}
            rows={6}
            value={form.content}
            onChange={(event) => update({ content: event.target.value })}
            placeholder="State the durable fact or instruction, including when it applies."
          />
        </div>

        <div className="grid gap-4 sm:grid-cols-2">
          {isCreate && (
            <div className="space-y-1.5">
              <Label htmlFor="knowledge-type">Type</Label>
              <Combobox
                id="knowledge-type"
                value={form.knowledgeType}
                onValueChange={(value) =>
                  update({ knowledgeType: value as KnowledgeType })
                }
                options={TYPE_OPTIONS}
                searchPlaceholder="Search types…"
              />
            </div>
          )}
          <div className="space-y-1.5">
            <Label htmlFor="knowledge-confidence">Confidence</Label>
            <Combobox
              id="knowledge-confidence"
              value={form.confidence}
              onValueChange={(value) =>
                update({ confidence: value as KnowledgeConfidence })
              }
              options={CONFIDENCE_OPTIONS}
            />
          </div>
        </div>

        {isCreate && (
          <div className="grid gap-4 sm:grid-cols-2">
            <div className="space-y-1.5">
              <Label htmlFor="knowledge-scope">Scope</Label>
              <Combobox
                id="knowledge-scope"
                value={form.scope}
                onValueChange={(value) =>
                  update({ scope: value as KnowledgeScope, agentId: '' })
                }
                options={SCOPE_OPTIONS}
              />
            </div>
            {form.scope !== 'workspace' && (
              <div className="space-y-1.5">
                <Label htmlFor="knowledge-board">Board</Label>
                <Combobox
                  id="knowledge-board"
                  value={form.boardId}
                  onValueChange={(value) => update({ boardId: value })}
                  options={boardOptions}
                  placeholder="Choose a board"
                  searchPlaceholder="Search boards…"
                  emptyText="No boards found."
                />
              </div>
            )}
            {form.scope === 'agent' && (
              <div className="space-y-1.5 sm:col-span-2">
                <Label htmlFor="knowledge-agent">Agent</Label>
                <Combobox
                  id="knowledge-agent"
                  value={form.agentId}
                  onValueChange={(value) => update({ agentId: value })}
                  options={agentOptions}
                  placeholder="Choose an agent"
                  searchPlaceholder="Search agents…"
                  emptyText="No enabled agents."
                />
              </div>
            )}
          </div>
        )}

        <FormError message={error} />
      </div>

      <DialogFooter className="mt-6">
        <DialogClose asChild>
          <Button type="button" variant="ghost">
            Cancel
          </Button>
        </DialogClose>
        <Button type="submit" loading={pending}>
          {copy.submit}
        </Button>
      </DialogFooter>
    </form>
  );
}

export function KnowledgeFormDialog({
  open,
  mode,
  item,
  onOpenChange,
}: {
  open: boolean;
  mode: KnowledgeFormMode;
  item: KnowledgeItem | null;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90vh] max-w-2xl overflow-y-auto">
        <KnowledgeForm mode={mode} item={item} onDone={() => onOpenChange(false)} />
      </DialogContent>
    </Dialog>
  );
}

function RejectForm({ item, onDone }: { item: KnowledgeItem; onDone: () => void }) {
  const toast = useToast();
  const reject = useRejectKnowledge();
  const [reason, setReason] = useState('');
  const [error, setError] = useState<string | null>(null);
  const reasonId = `reject-${item.id}`;

  async function submit(event: FormEvent) {
    event.preventDefault();
    setError(null);
    try {
      await reject.mutateAsync({
        id: item.id,
        expectedVersion: item.version,
        reason: reason.trim() || null,
      });
      toast.success('Candidate rejected.');
      onDone();
    } catch (cause) {
      setError(
        parseApiErrorMessage(
          cause,
          'Unable to update this knowledge item. Refresh and try again.',
        ),
      );
    }
  }

  return (
    <form onSubmit={(event) => void submit(event)}>
      <DialogHeader>
        <DialogTitle>Reject candidate</DialogTitle>
        <DialogDescription>
          “{item.title}” will not be used by agents. The reason is kept in the audit history.
        </DialogDescription>
      </DialogHeader>

      <div className="mt-5 space-y-3">
        <Label htmlFor={reasonId}>Reason (optional)</Label>
        <div
          role="group"
          aria-label="Reject reason presets"
          className="flex flex-wrap gap-1.5"
        >
          {REJECT_PRESETS.map((preset) => (
            <button
              key={preset.id}
              type="button"
              onClick={() => setReason(preset.reasonText)}
              className="rounded-md border border-danger-muted bg-surface-raised px-2 py-1 font-body text-xs font-medium text-danger transition-colors duration-fast hover:bg-danger-muted/40"
            >
              {preset.label}
            </button>
          ))}
        </div>
        <Textarea
          id={reasonId}
          rows={4}
          value={reason}
          onChange={(event) => setReason(event.target.value)}
          placeholder="Why should this candidate not be used?"
        />
        <FormError message={error} />
      </div>

      <DialogFooter className="mt-6">
        <DialogClose asChild>
          <Button type="button" variant="ghost">
            Cancel
          </Button>
        </DialogClose>
        <Button type="submit" variant="destructive" loading={reject.isPending}>
          Reject candidate
        </Button>
      </DialogFooter>
    </form>
  );
}

export function RejectKnowledgeDialog({
  open,
  item,
  onOpenChange,
}: {
  open: boolean;
  item: KnowledgeItem | null;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90vh] max-w-lg overflow-y-auto">
        {item && <RejectForm item={item} onDone={() => onOpenChange(false)} />}
      </DialogContent>
    </Dialog>
  );
}
