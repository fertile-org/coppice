import {
  BookOpen,
  Check,
  Clock3,
  ExternalLink,
  FileClock,
  GitBranch,
  Plus,
  ShieldCheck,
  Sparkles,
  X,
} from 'lucide-react';
import { useMemo, useState, type FormEvent } from 'react';
import { Button } from '../../components/ui/button';
import { Input } from '../../components/ui/input';
import { Label } from '../../components/ui/label';
import { Textarea } from '../../components/ui/textarea';
import { parseApiErrorMessage } from '../../lib/api';
import {
  knowledgeTypeSchema,
  type KnowledgeConfidence,
  type KnowledgeItem,
  type KnowledgeScope,
  type KnowledgeStatus,
  type KnowledgeType,
} from '../../lib/schemas/knowledge';
import { useAgents } from '../agents/useAgents';
import { useSession } from '../auth/useSession';
import { useProjects } from '../projects/useProjects';
import { useOpenTicket } from '../tickets/useOpenTicket';
import {
  useApproveKnowledge,
  useCreateKnowledge,
  useEditKnowledge,
  useExpireKnowledge,
  useKnowledge,
  useMarkKnowledgeStale,
  useRejectKnowledge,
  useSupersedeKnowledge,
  type KnowledgeRevisionInput,
} from './useKnowledge';

const STATUS_TABS: Array<{ value: KnowledgeStatus; label: string }> = [
  { value: 'pending', label: 'Pending' },
  { value: 'approved', label: 'Approved' },
  { value: 'rejected', label: 'Rejected' },
  { value: 'stale', label: 'Stale' },
];

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

const KNOWLEDGE_TYPES = knowledgeTypeSchema.options;

interface CandidateFormState {
  scope: KnowledgeScope;
  projectId: string;
  agentId: string;
  knowledgeType: KnowledgeType;
  title: string;
  content: string;
  confidence: KnowledgeConfidence;
}

const EMPTY_CANDIDATE: CandidateFormState = {
  scope: 'project',
  projectId: '',
  agentId: '',
  knowledgeType: 'coding_convention',
  title: '',
  content: '',
  confidence: 'medium',
};

function formatDate(value: string | null): string {
  if (!value) return 'Never';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  });
}

function humanize(value: string): string {
  return value
    .split('_')
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ');
}

function shortId(value: string): string {
  return value.slice(0, 8);
}

function statusPillClass(status: KnowledgeStatus): string {
  const base =
    'rounded-full border px-2 py-0.5 font-body text-xs font-medium';
  switch (status) {
    case 'pending':
      return `${base} border-warning-muted bg-warning-muted text-warning`;
    case 'approved':
      return `${base} border-success-muted bg-success-muted text-success`;
    case 'rejected':
      return `${base} border-danger-muted bg-danger-muted text-danger`;
    case 'stale':
      return `${base} border-border bg-paper-200 text-text-secondary`;
  }
}

function embeddingPillClass(status: string): string {
  const base = 'rounded-full px-2 py-0.5 font-body text-xs font-medium';
  if (status === 'ready') return `${base} bg-moss-100 text-moss-800`;
  if (status === 'failed') return `${base} bg-danger-muted text-danger`;
  if (status === 'processing' || status === 'pending') {
    return `${base} bg-info-muted text-info`;
  }
  return `${base} bg-paper-200 text-text-muted`;
}

function candidateInput(form: CandidateFormState): KnowledgeRevisionInput {
  return {
    scope: form.scope,
    projectId: form.scope === 'workspace' ? null : form.projectId,
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

function ManualCandidateForm() {
  const { data: projects } = useProjects();
  const { data: agents } = useAgents();
  const create = useCreateKnowledge();
  const [form, setForm] = useState<CandidateFormState>(EMPTY_CANDIDATE);
  const [error, setError] = useState<string | null>(null);

  const availableAgents = useMemo(
    () => agents?.filter((agent) => agent.enabled) ?? [],
    [agents],
  );

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (form.scope !== 'workspace' && !form.projectId) {
      setError('Choose a project for this scope.');
      return;
    }
    if (form.scope === 'agent' && !form.agentId) {
      setError('Choose an agent for agent-scoped knowledge.');
      return;
    }
    if (!form.title.trim() || !form.content.trim()) {
      setError('Title and content are required.');
      return;
    }
    setError(null);
    try {
      await create.mutateAsync(candidateInput(form));
      setForm((previous) => ({
        ...EMPTY_CANDIDATE,
        projectId: previous.projectId,
      }));
    } catch (cause) {
      setError(parseApiErrorMessage(cause, 'Unable to create candidate.'));
    }
  }

  return (
    <form
      onSubmit={(event) => void submit(event)}
      className="rounded-xl border border-border bg-surface-raised p-5 shadow-card"
    >
      <div className="flex items-center gap-2">
        <Plus className="size-4 text-moss-600" aria-hidden="true" />
        <h2 className="font-display text-lg font-semibold text-bark-900">
          Manual candidate
        </h2>
      </div>
      <p className="mt-1 font-body text-sm text-text-secondary">
        New notes begin in Pending so their scope and wording can be reviewed.
      </p>

      <div className="mt-5 space-y-4">
        <div className="space-y-1.5">
          <Label htmlFor="knowledge-title">Title</Label>
          <Input
            id="knowledge-title"
            required
            maxLength={160}
            value={form.title}
            onChange={(event) =>
              setForm((value) => ({ ...value, title: event.target.value }))
            }
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
            onChange={(event) =>
              setForm((value) => ({ ...value, content: event.target.value }))
            }
            placeholder="State the durable fact or instruction, including when it applies."
          />
        </div>

        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-1">
          <div className="space-y-1.5">
            <Label htmlFor="knowledge-type">Type</Label>
            <select
              id="knowledge-type"
              className="field-control w-full px-3 py-2 font-body text-sm"
              value={form.knowledgeType}
              onChange={(event) =>
                setForm((value) => ({
                  ...value,
                  knowledgeType: event.target.value as KnowledgeType,
                }))
              }
            >
              {KNOWLEDGE_TYPES.map((type) => (
                <option key={type} value={type}>
                  {TYPE_LABELS[type]}
                </option>
              ))}
            </select>
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="knowledge-confidence">Confidence</Label>
            <select
              id="knowledge-confidence"
              className="field-control w-full px-3 py-2 font-body text-sm"
              value={form.confidence}
              onChange={(event) =>
                setForm((value) => ({
                  ...value,
                  confidence: event.target.value as KnowledgeConfidence,
                }))
              }
            >
              <option value="low">Low</option>
              <option value="medium">Medium</option>
              <option value="high">High</option>
            </select>
          </div>
        </div>

        <div className="space-y-1.5">
          <Label htmlFor="knowledge-scope">Scope</Label>
          <select
            id="knowledge-scope"
            className="field-control w-full px-3 py-2 font-body text-sm"
            value={form.scope}
            onChange={(event) =>
              setForm((value) => ({
                ...value,
                scope: event.target.value as KnowledgeScope,
                agentId: '',
              }))
            }
          >
            <option value="workspace">Workspace</option>
            <option value="project">Project</option>
            <option value="agent">Project + agent</option>
          </select>
        </div>

        {form.scope !== 'workspace' && (
          <div className="space-y-1.5">
            <Label htmlFor="knowledge-project">Project</Label>
            <select
              id="knowledge-project"
              required
              className="field-control w-full px-3 py-2 font-body text-sm"
              value={form.projectId}
              onChange={(event) =>
                setForm((value) => ({
                  ...value,
                  projectId: event.target.value,
                }))
              }
            >
              <option value="">Choose a project</option>
              {projects?.map((project) => (
                <option key={project.id} value={project.id}>
                  {project.name}
                </option>
              ))}
            </select>
          </div>
        )}

        {form.scope === 'agent' && (
          <div className="space-y-1.5">
            <Label htmlFor="knowledge-agent">Agent</Label>
            <select
              id="knowledge-agent"
              required
              className="field-control w-full px-3 py-2 font-body text-sm"
              value={form.agentId}
              onChange={(event) =>
                setForm((value) => ({
                  ...value,
                  agentId: event.target.value,
                }))
              }
            >
              <option value="">Choose an agent</option>
              {availableAgents.map((agent) => (
                <option key={agent.id} value={agent.id}>
                  {agent.name}
                </option>
              ))}
            </select>
          </div>
        )}
      </div>

      {error && (
        <p
          role="alert"
          className="mt-4 rounded-md bg-danger-muted px-3 py-2 font-body text-sm text-danger"
        >
          {error}
        </p>
      )}

      <Button type="submit" loading={create.isPending} className="mt-5 w-full">
        Add to Pending
      </Button>
    </form>
  );
}

type EditorMode = 'edit' | 'supersede' | 'reject' | null;

function KnowledgeCard({
  item,
  canGovern,
  onOpenTicket,
}: {
  item: KnowledgeItem;
  canGovern: boolean;
  onOpenTicket: (ticketId: string) => void | Promise<void>;
}) {
  const approve = useApproveKnowledge();
  const reject = useRejectKnowledge();
  const edit = useEditKnowledge();
  const supersede = useSupersedeKnowledge();
  const markStale = useMarkKnowledgeStale();
  const expire = useExpireKnowledge();
  const [mode, setMode] = useState<EditorMode>(null);
  const [title, setTitle] = useState(item.title);
  const [content, setContent] = useState(item.content);
  const [confidence, setConfidence] =
    useState<KnowledgeConfidence>(item.confidence);
  const [reason, setReason] = useState('');
  const [error, setError] = useState<string | null>(null);

  const busy =
    approve.isPending ||
    reject.isPending ||
    edit.isPending ||
    supersede.isPending ||
    markStale.isPending ||
    expire.isPending;

  const scopeLabel =
    item.scope === 'workspace'
      ? 'Workspace'
      : item.scope === 'agent'
        ? `${item.projectName ?? 'Project'} · ${item.agentName ?? 'Agent'}`
        : item.projectName ?? 'Project';
  const hasExpiry = item.expiresAt !== null;
  const awaitingReplacementEmbedding =
    item.status === 'approved' &&
    item.activeRevisionId !== null &&
    item.activeRevisionId !== item.revisionId;
  const sourceCanOpen =
    (item.sourceType === 'ticket' || item.sourceType === 'agent_summary') &&
    item.sourceId;

  async function run(action: () => Promise<unknown>) {
    setError(null);
    try {
      await action();
      setMode(null);
    } catch (cause) {
      setError(
        parseApiErrorMessage(
          cause,
          'Unable to update this knowledge item. Refresh and try again.',
        ),
      );
    }
  }

  function openEditor(nextMode: Exclude<EditorMode, 'reject' | null>) {
    setTitle(item.title);
    setContent(item.content);
    setConfidence(item.confidence);
    setError(null);
    setMode(nextMode);
  }

  async function saveEditor(event: FormEvent) {
    event.preventDefault();
    if (!title.trim() || !content.trim()) {
      setError('Title and content are required.');
      return;
    }
    if (mode === 'edit') {
      await run(() =>
        edit.mutateAsync({
          id: item.id,
          expectedVersion: item.version,
          patch: {
            title: title.trim(),
            content: content.trim(),
            confidence,
          },
        }),
      );
      return;
    }
    if (mode === 'supersede') {
      await run(() =>
        supersede.mutateAsync({
          id: item.id,
          expectedVersion: item.version,
          replacement: {
            scope: item.scope,
            projectId: item.projectId,
            agentId: item.agentId,
            knowledgeType: item.knowledgeType,
            title: title.trim(),
            content: content.trim(),
            sourceType: 'human_note',
            sourceId: null,
            sourceRunId: null,
            confidence,
          },
        }),
      );
    }
  }

  return (
    <article className="rounded-xl border border-border bg-surface-raised p-5 shadow-card">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <span className={statusPillClass(item.status)}>
              {humanize(item.status)}
            </span>
            <span className="font-body text-xs font-medium uppercase tracking-wide text-moss-700">
              {TYPE_LABELS[item.knowledgeType]}
            </span>
            <span className="font-body text-xs text-text-muted">
              {humanize(item.confidence)} confidence
            </span>
          </div>
          <h2 className="mt-2 font-display text-xl font-semibold text-bark-900">
            {item.title}
          </h2>
          <p className="mt-2 font-body text-sm leading-relaxed text-bark-700 whitespace-pre-wrap">
            {item.content}
          </p>
        </div>
        <span className={embeddingPillClass(item.embeddingStatus)}>
          Embedding · {humanize(item.embeddingStatus)}
        </span>
      </div>

      {awaitingReplacementEmbedding && (
        <div className="mt-4 flex gap-2 rounded-md border border-info-muted bg-info-muted/60 px-3 py-2">
          <ShieldCheck className="mt-0.5 size-4 shrink-0 text-info" aria-hidden="true" />
          <p className="font-body text-xs text-info">
            Revision {item.revisionNumber} is being embedded. The previous revision remains active until it is ready.
          </p>
        </div>
      )}

      <dl className="mt-4 grid gap-x-5 gap-y-3 border-t border-border pt-4 sm:grid-cols-2 xl:grid-cols-3">
        <div>
          <dt className="font-body text-xs uppercase tracking-wide text-text-muted">Scope</dt>
          <dd className="mt-0.5 font-body text-sm text-text-secondary">{scopeLabel}</dd>
        </div>
        <div>
          <dt className="font-body text-xs uppercase tracking-wide text-text-muted">Source</dt>
          <dd className="mt-0.5 flex items-center gap-2 font-body text-sm text-text-secondary">
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
          </dd>
        </div>
        <div>
          <dt className="font-body text-xs uppercase tracking-wide text-text-muted">Revision</dt>
          <dd className="mt-0.5 font-body text-sm text-text-secondary">
            {item.revisionNumber} · version {item.version}
          </dd>
        </div>
        <div>
          <dt className="font-body text-xs uppercase tracking-wide text-text-muted">Expiry</dt>
          <dd className="mt-0.5 font-body text-sm text-text-secondary">
            {formatDate(item.expiresAt)}
          </dd>
        </div>
        <div>
          <dt className="font-body text-xs uppercase tracking-wide text-text-muted">Usage</dt>
          <dd className="mt-0.5 font-body text-sm text-text-secondary">
            {item.usageCount} runs · last {formatDate(item.lastUsedAt)}
          </dd>
        </div>
        <div>
          <dt className="font-body text-xs uppercase tracking-wide text-text-muted">Updated</dt>
          <dd className="mt-0.5 font-body text-sm text-text-secondary">{formatDate(item.updatedAt)}</dd>
        </div>
      </dl>

      {(item.supersedesItemId || item.supersededBy || item.sourceRunId) && (
        <div className="mt-4 flex flex-wrap gap-x-5 gap-y-1 rounded-md bg-paper-100 px-3 py-2 font-mono text-xs text-text-secondary">
          {item.supersedesItemId && (
            <span>Supersedes {shortId(item.supersedesItemId)}</span>
          )}
          {item.supersededBy && (
            <span>Superseded by {shortId(item.supersededBy)}</span>
          )}
          {item.sourceRunId && <span>Source run {shortId(item.sourceRunId)}</span>}
        </div>
      )}

      {(item.policyReason || item.rejectionReason || item.embeddingError) && (
        <div className="mt-3 space-y-1 rounded-md border border-border bg-paper-50 px-3 py-2 font-body text-xs text-text-secondary">
          {item.policyReason && (
            <p><span className="font-medium">Policy:</span> {item.policyReason}</p>
          )}
          {item.rejectionReason && (
            <p><span className="font-medium">Rejected:</span> {item.rejectionReason}</p>
          )}
          {item.embeddingError && (
            <p className="text-danger"><span className="font-medium">Embedding:</span> {item.embeddingError}</p>
          )}
        </div>
      )}

      {canGovern && !item.supersededBy && (
        <div className="mt-4 flex flex-wrap gap-2 border-t border-border pt-4">
          {item.status !== 'approved' && (
            <Button
              type="button"
              size="sm"
              loading={approve.isPending}
              disabled={busy}
              onClick={() => void run(() => approve.mutateAsync({ id: item.id, expectedVersion: item.version }))}
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
            onClick={() => openEditor('edit')}
          >
            Edit
          </Button>
          {item.status === 'pending' && (
            <Button
              type="button"
              variant="destructive"
              size="sm"
              disabled={busy}
              onClick={() => {
                setReason('');
                setError(null);
                setMode('reject');
              }}
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
              onClick={() => openEditor('supersede')}
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
              onClick={() => void run(() => markStale.mutateAsync({ id: item.id, expectedVersion: item.version }))}
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
              onClick={() => void run(() => expire.mutateAsync({ id: item.id, expectedVersion: item.version }))}
            >
              <Clock3 className="size-3.5" aria-hidden="true" />
              Expire now
            </Button>
          )}
        </div>
      )}

      {(mode === 'edit' || mode === 'supersede') && (
        <form onSubmit={(event) => void saveEditor(event)} className="mt-4 space-y-3 rounded-lg border border-moss-200 bg-moss-50 p-4">
          <div>
            <h3 className="font-display text-sm font-semibold text-bark-900">
              {mode === 'edit' ? 'Create a new revision' : 'Create replacement candidate'}
            </h3>
            <p className="mt-0.5 font-body text-xs text-text-secondary">
              {mode === 'edit'
                ? 'The previous revision remains in the audit history.'
                : 'The current item stays usable until the replacement is approved and embedding-ready.'}
            </p>
          </div>
          <Input
            aria-label="Revision title"
            required
            maxLength={160}
            value={title}
            onChange={(event) => setTitle(event.target.value)}
          />
          <Textarea
            aria-label="Revision content"
            required
            maxLength={12000}
            rows={5}
            value={content}
            onChange={(event) => setContent(event.target.value)}
          />
          <select
            aria-label="Revision confidence"
            className="field-control w-full px-3 py-2 font-body text-sm"
            value={confidence}
            onChange={(event) => setConfidence(event.target.value as KnowledgeConfidence)}
          >
            <option value="low">Low confidence</option>
            <option value="medium">Medium confidence</option>
            <option value="high">High confidence</option>
          </select>
          <div className="flex gap-2">
            <Button type="submit" size="sm" loading={edit.isPending || supersede.isPending}>
              {mode === 'edit' ? 'Save revision' : 'Create replacement'}
            </Button>
            <Button type="button" variant="ghost" size="sm" onClick={() => setMode(null)}>
              Cancel
            </Button>
          </div>
        </form>
      )}

      {mode === 'reject' && (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            void run(() => reject.mutateAsync({
              id: item.id,
              expectedVersion: item.version,
              reason: reason.trim() || null,
            }));
          }}
          className="mt-4 space-y-3 rounded-lg border border-danger-muted bg-danger-muted/30 p-4"
        >
          <Label htmlFor={`reject-${item.id}`}>Reason (optional)</Label>
          <Textarea
            id={`reject-${item.id}`}
            value={reason}
            onChange={(event) => setReason(event.target.value)}
            placeholder="Why should this candidate not be used?"
          />
          <div className="flex gap-2">
            <Button type="submit" variant="destructive" size="sm" loading={reject.isPending}>
              Reject candidate
            </Button>
            <Button type="button" variant="ghost" size="sm" onClick={() => setMode(null)}>
              Cancel
            </Button>
          </div>
        </form>
      )}

      {error && (
        <p role="alert" className="mt-3 rounded-md bg-danger-muted px-3 py-2 font-body text-sm text-danger">
          {error}
        </p>
      )}
    </article>
  );
}

export function KnowledgePage() {
  const { user } = useSession();
  const { data: projects } = useProjects();
  const openTicket = useOpenTicket();
  const [status, setStatus] = useState<KnowledgeStatus>('pending');
  const [projectId, setProjectId] = useState('');
  const [knowledgeType, setKnowledgeType] = useState('');
  const query = useKnowledge({
    status,
    projectId: projectId || undefined,
    knowledgeType: (knowledgeType || undefined) as KnowledgeType | undefined,
  });
  const items = query.data?.pages.flatMap((page) => page.items) ?? [];

  return (
    <div>
      <header className="flex flex-wrap items-end justify-between gap-5 border-b border-border pb-6">
        <div className="max-w-2xl">
          <div className="flex items-center gap-2 font-body text-xs font-medium uppercase tracking-[0.16em] text-moss-700">
            <Sparkles className="size-4" aria-hidden="true" />
            Governed memory
          </div>
          <h1 className="mt-2 font-display text-3xl font-semibold tracking-tight text-bark-950">
            Knowledge
          </h1>
          <p className="mt-2 font-body text-sm leading-relaxed text-text-secondary">
            Review durable facts before agents can use them. Every revision keeps its source, policy decision, embedding state, and run history.
          </p>
        </div>
        <div className="flex items-center gap-2 rounded-lg border border-moss-200 bg-moss-50 px-3 py-2">
          <ShieldCheck className="size-4 text-moss-700" aria-hidden="true" />
          <span className="font-body text-xs font-medium text-moss-800">
            Human-governed · fail-closed
          </span>
        </div>
      </header>

      <div className="mt-6 grid items-start gap-6 lg:grid-cols-[minmax(0,1fr)_19rem]">
        <section aria-label="Knowledge library" className="min-w-0">
          <div className="rounded-xl border border-border bg-surface p-2 shadow-sm">
            <div role="tablist" aria-label="Knowledge status" className="grid grid-cols-4 gap-1">
              {STATUS_TABS.map((tab) => (
                <button
                  key={tab.value}
                  type="button"
                  role="tab"
                  aria-selected={status === tab.value}
                  onClick={() => setStatus(tab.value)}
                  className={
                    status === tab.value
                      ? 'rounded-md bg-surface-raised px-3 py-2 font-body text-sm font-medium text-bark-900 shadow-sm'
                      : 'rounded-md px-3 py-2 font-body text-sm text-text-secondary transition-colors hover:bg-paper-200 hover:text-bark-900'
                  }
                >
                  {tab.label}
                </button>
              ))}
            </div>
          </div>

          <div className="mt-4 flex flex-wrap gap-3">
            <label className="min-w-48 flex-1 font-body text-xs font-medium text-text-secondary">
              Project
              <select
                className="field-control mt-1 block w-full px-3 py-2 font-body text-sm"
                value={projectId}
                onChange={(event) => setProjectId(event.target.value)}
              >
                <option value="">All scopes</option>
                {projects?.map((project) => (
                  <option key={project.id} value={project.id}>{project.name}</option>
                ))}
              </select>
            </label>
            <label className="min-w-48 flex-1 font-body text-xs font-medium text-text-secondary">
              Type
              <select
                className="field-control mt-1 block w-full px-3 py-2 font-body text-sm"
                value={knowledgeType}
                onChange={(event) => setKnowledgeType(event.target.value)}
              >
                <option value="">All types</option>
                {KNOWLEDGE_TYPES.map((type) => (
                  <option key={type} value={type}>{TYPE_LABELS[type]}</option>
                ))}
              </select>
            </label>
          </div>

          {query.isLoading && (
            <div className="mt-5 rounded-xl border border-dashed border-border bg-paper-50 p-10 text-center">
              <BookOpen className="mx-auto size-6 text-text-muted" aria-hidden="true" />
              <p className="mt-2 font-body text-sm text-text-muted">Loading knowledge…</p>
            </div>
          )}
          {query.isError && (
            <div className="mt-5 rounded-xl border border-danger-muted bg-danger-muted/30 p-5">
              <p className="font-body text-sm text-danger">Unable to load knowledge.</p>
              <Button type="button" variant="secondary" size="sm" className="mt-3" onClick={() => void query.refetch()}>
                Try again
              </Button>
            </div>
          )}
          {!query.isLoading && !query.isError && items.length === 0 && (
            <div className="mt-5 rounded-xl border border-dashed border-border bg-paper-50 p-10 text-center">
              <BookOpen className="mx-auto size-7 text-moss-500" aria-hidden="true" />
              <h2 className="mt-3 font-display text-lg font-semibold text-bark-800">
                No {status} knowledge
              </h2>
              <p className="mt-1 font-body text-sm text-text-muted">
                {status === 'pending'
                  ? 'New manual and extracted candidates will wait here for review.'
                  : 'Items will appear here as their lifecycle changes.'}
              </p>
            </div>
          )}
          {items.length > 0 && (
            <div className="mt-5 space-y-4">
              {items.map((item) => (
                <KnowledgeCard
                  key={item.id}
                  item={item}
                  canGovern={user?.role === 'admin'}
                  onOpenTicket={openTicket}
                />
              ))}
            </div>
          )}
          {query.hasNextPage && (
            <div className="mt-5 flex justify-center">
              <Button
                type="button"
                variant="secondary"
                loading={query.isFetchingNextPage}
                onClick={() => void query.fetchNextPage()}
              >
                Load more
              </Button>
            </div>
          )}
        </section>

        <aside className="lg:sticky lg:top-6">
          {user?.role === 'admin' ? (
            <ManualCandidateForm />
          ) : (
            <div className="rounded-xl border border-border bg-surface p-5">
              <ShieldCheck className="size-5 text-moss-600" aria-hidden="true" />
              <h2 className="mt-2 font-display text-base font-semibold text-bark-900">Read-only knowledge</h2>
              <p className="mt-1 font-body text-sm text-text-secondary">
                An administrator can create candidates and govern their lifecycle.
              </p>
            </div>
          )}
        </aside>
      </div>
    </div>
  );
}
