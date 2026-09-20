import { MessageSquarePlus, Paperclip, X } from 'lucide-react';
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent,
} from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { Button } from '../../components/ui/button';
import { Label } from '../../components/ui/label';
import { Textarea } from '../../components/ui/textarea';
import { parseApiErrorMessage } from '../../lib/api';
import {
  formatFileSize,
  isImageContentType,
} from '../../lib/attachments';
import type { ChatSession, ChatSessionStatus } from '../../lib/schemas/chat';
import { cn } from '../../lib/utils';
import { useAgents } from '../agents/useAgents';
import { useProjects } from '../projects/useProjects';
import { useUploadAttachment } from '../tickets/useTicket';
import { ChatLiveTurn } from './ChatLiveTurn';
import { ChatMessageList } from './ChatMessageList';
import { ChatSessionActions } from './ChatSessionActions';
import {
  useChatMessages,
  useChatSession,
  useChatSessions,
  useCreateChatSession,
  usePostChatMessage,
} from './useChat';

const CHAT_ATTACHMENT_ACCEPT =
  'image/png,image/jpeg,image/gif,image/webp,text/plain,text/markdown,text/csv,application/json,application/pdf,.md,.csv,.json,.pdf,.txt,.png,.jpg,.jpeg,.gif,.webp';
const MAX_CHAT_ATTACHMENTS = 5;

interface PendingFile {
  key: string;
  file: File;
  previewUrl: string | null;
}

function pendingFileFromFile(file: File): PendingFile {
  return {
    key: `${file.name}-${file.size}-${file.lastModified}-${crypto.randomUUID()}`,
    file,
    previewUrl: isImageContentType(file.type)
      ? URL.createObjectURL(file)
      : null,
  };
}

function formatSessionTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '';
  return date.toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  });
}

function sessionStatusLabel(status: ChatSessionStatus): string {
  switch (status) {
    case 'active':
      return 'Active';
    case 'cutoff':
      return 'Cutoff';
    case 'archived':
      return 'Archived';
  }
}

function SessionStatusPill({ status }: { status: ChatSessionStatus }) {
  return (
    <span
      className={cn(
        'inline-flex items-center rounded-full px-2 py-0.5 font-body text-[11px] font-medium',
        status === 'active' && 'bg-moss-100 text-moss-800',
        status === 'cutoff' && 'bg-warning-muted text-bark-700',
        status === 'archived' && 'bg-bark-100 text-bark-600',
      )}
      data-testid="chat-session-status"
    >
      {sessionStatusLabel(status)}
    </span>
  );
}

function SessionListItem({
  session,
  agentName,
  active,
}: {
  session: ChatSession;
  agentName: string;
  active: boolean;
}) {
  return (
    <Link
      to={`/chat/${session.id}`}
      className={[
        'block rounded-md border px-3 py-2 transition-colors duration-fast',
        active
          ? 'border-accent bg-accent-muted text-accent'
          : 'border-transparent hover:border-border hover:bg-paper-100',
      ].join(' ')}
    >
      <div className="font-body text-sm font-medium text-text-primary">
        {agentName}
      </div>
      <div className="mt-0.5 font-body text-xs text-text-secondary">
        {formatSessionTime(session.updatedAt)} · {session.status}
      </div>
    </Link>
  );
}

function NewChatForm({
  onCreated,
}: {
  onCreated: (sessionId: string) => void;
}) {
  const { data: agents = [] } = useAgents();
  const { data: projects = [] } = useProjects();
  const createSession = useCreateChatSession();
  const enabledAgents = useMemo(
    () => agents.filter((agent) => agent.enabled),
    [agents],
  );
  const [agentId, setAgentId] = useState('');
  const [projectId, setProjectId] = useState('');
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    setError(null);
    if (!agentId) {
      setError('Select an agent.');
      return;
    }
    try {
      const session = await createSession.mutateAsync({
        agentId,
        projectId: projectId || null,
      });
      onCreated(session.id);
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Could not create chat.'));
    }
  }

  return (
    <form
      onSubmit={(event) => void handleSubmit(event)}
      className="space-y-4 rounded-lg border border-border bg-surface-raised p-4"
      data-testid="new-chat-form"
    >
      <div>
        <h2 className="font-display text-lg font-semibold text-text-primary">
          New chat
        </h2>
        <p className="mt-1 font-body text-sm text-text-secondary">
          Talk to an agent outside a ticket thread.
        </p>
      </div>

      <div className="space-y-2">
        <Label htmlFor="chat-agent">Agent</Label>
        <select
          id="chat-agent"
          aria-label="Agent"
          className="field-control h-10 w-full px-3 font-body text-sm"
          value={agentId}
          onChange={(event) => setAgentId(event.target.value)}
        >
          <option value="">Select agent</option>
          {enabledAgents.map((agent) => (
            <option key={agent.id} value={agent.id}>
              {agent.name}
            </option>
          ))}
        </select>
      </div>

      <div className="space-y-2">
        <Label htmlFor="chat-project">Project (optional)</Label>
        <select
          id="chat-project"
          aria-label="Project (optional)"
          className="field-control h-10 w-full px-3 font-body text-sm"
          value={projectId}
          onChange={(event) => setProjectId(event.target.value)}
        >
          <option value="">No project</option>
          {projects.map((project) => (
            <option key={project.id} value={project.id}>
              {project.name}
            </option>
          ))}
        </select>
      </div>

      {error && (
        <p className="font-body text-sm text-danger" role="alert">
          {error}
        </p>
      )}

      <Button type="submit" disabled={createSession.isPending}>
        {createSession.isPending ? 'Starting…' : 'Start chat'}
      </Button>
    </form>
  );
}

function ChatComposer({
  sessionId,
  disabled,
  onPosted,
}: {
  sessionId: string;
  disabled?: boolean;
  onPosted: (runId: string) => void;
}) {
  const postMessage = usePostChatMessage(sessionId);
  const uploadAttachment = useUploadAttachment();
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [body, setBody] = useState('');
  const [pendingFiles, setPendingFiles] = useState<PendingFile[]>([]);
  const [error, setError] = useState<string | null>(null);
  const composerLocked = Boolean(
    disabled || postMessage.isPending || uploadAttachment.isPending,
  );
  const canSend = body.trim().length > 0 || pendingFiles.length > 0;

  const pendingFilesRef = useRef(pendingFiles);
  pendingFilesRef.current = pendingFiles;

  useEffect(() => {
    return () => {
      for (const pending of pendingFilesRef.current) {
        if (pending.previewUrl) {
          URL.revokeObjectURL(pending.previewUrl);
        }
      }
    };
  }, []);

  function addPendingFiles(files: FileList | File[]) {
    const next = Array.from(files).map(pendingFileFromFile);
    if (next.length === 0) return;
    setPendingFiles((current) => {
      const room = MAX_CHAT_ATTACHMENTS - current.length;
      if (room <= 0) return current;
      return [...current, ...next.slice(0, room)];
    });
  }

  function removePendingFile(key: string) {
    setPendingFiles((current) => {
      const removed = current.find((item) => item.key === key);
      if (removed?.previewUrl) {
        URL.revokeObjectURL(removed.previewUrl);
      }
      return current.filter((item) => item.key !== key);
    });
  }

  function clearPendingFiles() {
    setPendingFiles((current) => {
      for (const pending of current) {
        if (pending.previewUrl) {
          URL.revokeObjectURL(pending.previewUrl);
        }
      }
      return [];
    });
    if (fileInputRef.current) fileInputRef.current.value = '';
  }

  async function submitMessage() {
    const trimmed = body.trim();
    if ((!trimmed && pendingFiles.length === 0) || composerLocked) return;
    setError(null);
    try {
      const attachmentIds: string[] = [];
      for (const pending of pendingFiles) {
        const uploaded = await uploadAttachment.mutateAsync(pending.file);
        attachmentIds.push(uploaded.id);
      }
      const result = await postMessage.mutateAsync({
        body: trimmed,
        attachmentIds: attachmentIds.length > 0 ? attachmentIds : undefined,
      });
      setBody('');
      clearPendingFiles();
      onPosted(result.runId);
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Could not send message.'));
    }
  }

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    await submitMessage();
  }

  function handleKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key !== 'Enter') return;
    if (event.nativeEvent.isComposing || event.keyCode === 229) return;
    if (event.shiftKey) return;
    event.preventDefault();
    void submitMessage();
  }

  return (
    <form
      onSubmit={(event) => void handleSubmit(event)}
      className="shrink-0 border-t border-border bg-paper-50/90 px-3 py-2.5"
      data-testid="chat-composer"
    >
      {pendingFiles.length > 0 && (
        <ul className="mb-2 flex flex-wrap gap-2">
          {pendingFiles.map((pending) => (
            <li key={pending.key} className="relative">
              {pending.previewUrl ? (
                <img
                  src={pending.previewUrl}
                  alt={pending.file.name}
                  className="size-14 rounded-md border border-border object-cover"
                />
              ) : (
                <div className="flex size-14 flex-col items-center justify-center rounded-md border border-border bg-surface px-1 text-center">
                  <span className="line-clamp-2 font-body text-[10px] leading-tight text-text-secondary">
                    {pending.file.name}
                  </span>
                  <span className="font-body text-[10px] text-text-muted">
                    {formatFileSize(pending.file.size)}
                  </span>
                </div>
              )}
              <button
                type="button"
                onClick={() => removePendingFile(pending.key)}
                className="absolute -right-1.5 -top-1.5 rounded-full border border-border bg-surface-raised p-0.5 text-text-muted shadow-sm hover:text-text-primary"
                aria-label={`Remove ${pending.file.name}`}
                disabled={composerLocked}
              >
                <X className="size-3" />
              </button>
            </li>
          ))}
        </ul>
      )}

      <div className="flex items-end gap-2">
        <input
          ref={fileInputRef}
          id="chat-composer-files"
          type="file"
          multiple
          accept={CHAT_ATTACHMENT_ACCEPT}
          className="sr-only"
          data-testid="chat-composer-files"
          tabIndex={-1}
          disabled={composerLocked || pendingFiles.length >= MAX_CHAT_ATTACHMENTS}
          onChange={(event) => {
            if (event.target.files) {
              addPendingFiles(event.target.files);
            }
            event.target.value = '';
          }}
        />
        <Button
          type="button"
          variant="secondary"
          className="shrink-0 px-2.5"
          disabled={composerLocked || pendingFiles.length >= MAX_CHAT_ATTACHMENTS}
          aria-label="Attach files"
          onClick={() => fileInputRef.current?.click()}
        >
          <Paperclip className="size-4" />
        </Button>
        <Label htmlFor="chat-composer-input" className="sr-only">
          Message
        </Label>
        <Textarea
          id="chat-composer-input"
          value={body}
          onChange={(event) => setBody(event.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Message the agent…"
          rows={2}
          disabled={composerLocked}
          className="min-h-[2.75rem] flex-1 resize-none border-border bg-surface-raised"
          aria-describedby="chat-composer-hint"
        />
        <Button
          type="submit"
          disabled={composerLocked || !canSend}
          className="shrink-0"
        >
          {postMessage.isPending || uploadAttachment.isPending
            ? 'Sending…'
            : 'Send'}
        </Button>
      </div>
      <p
        id="chat-composer-hint"
        className="mt-1.5 font-body text-[11px] text-text-muted"
      >
        Enter to send · Shift+Enter for a new line · up to {MAX_CHAT_ATTACHMENTS}{' '}
        files
      </p>
      {error && (
        <p className="mt-1 font-body text-sm text-danger" role="alert">
          {error}
        </p>
      )}
    </form>
  );
}

function ChatSessionPane({ sessionId }: { sessionId: string }) {
  const { data: agents = [] } = useAgents();
  const { data: session } = useChatSession(sessionId);
  const [activeRunId, setActiveRunId] = useState<string | null>(null);
  const awaiting = Boolean(activeRunId);

  const { data: messages = [], refetch } = useChatMessages(sessionId, {
    refetchInterval: awaiting ? 1500 : false,
  });

  const agentName =
    agents.find((agent) => agent.id === session?.agentId)?.name ?? 'Agent';

  const onLiveFinished = useCallback(() => {
    setActiveRunId(null);
    void refetch();
  }, [refetch]);

  const cutoff = session?.status === 'cutoff' || session?.status === 'archived';

  return (
    <div
      className="flex h-[min(70vh,720px)] flex-col overflow-hidden rounded-xl border border-border bg-surface-raised shadow-sm"
      data-testid="chat-session-pane"
    >
      <header className="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-border bg-paper-50/80 px-4 py-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <h2 className="font-display text-base font-semibold text-text-primary">
              {agentName}
            </h2>
            {session ? <SessionStatusPill status={session.status} /> : null}
          </div>
          <p className="mt-0.5 font-body text-xs text-text-secondary">
            {session
              ? `Updated ${formatSessionTime(session.updatedAt)}`
              : 'Loading conversation…'}
          </p>
        </div>
        {session && (
          <ChatSessionActions session={session} disabled={awaiting} />
        )}
      </header>

      <div className="flex min-h-0 flex-1 flex-col gap-2 bg-paper-100/40 px-3 py-2">
        <ChatMessageList
          messages={messages}
          thinking={awaiting && !activeRunId}
        />
        {activeRunId ? (
          <ChatLiveTurn runId={activeRunId} onFinished={onLiveFinished} />
        ) : null}
      </div>

      {cutoff ? (
        <p className="shrink-0 border-t border-border bg-paper-50/90 px-4 py-3 font-body text-sm text-text-secondary">
          This session is {session?.status}. History stays readable
          {session?.status === 'cutoff'
            ? ' — open the continued child session from the transcript chip if one exists.'
            : '.'}{' '}
          <Link to="/chat" className="text-accent hover:underline">
            Start a new chat
          </Link>
        </p>
      ) : (
        <ChatComposer
          sessionId={sessionId}
          disabled={awaiting}
          onPosted={(runId) => setActiveRunId(runId)}
        />
      )}
    </div>
  );
}

export function ChatPage() {
  const { sessionId } = useParams<{ sessionId?: string }>();
  const navigate = useNavigate();
  const { data: agents = [] } = useAgents();
  const { data: sessions = [], isLoading } = useChatSessions();
  const [showNew, setShowNew] = useState(!sessionId);

  const agentNameById = useMemo(() => {
    const map = new Map<string, string>();
    for (const agent of agents) map.set(agent.id, agent.name);
    return map;
  }, [agents]);

  return (
    <div className="space-y-6" data-testid="chat-page">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h1 className="font-display text-2xl font-semibold text-text-primary">
            Chat
          </h1>
          <p className="mt-1 font-body text-sm text-text-secondary">
            Human-owned exploratory conversations with a chosen agent.
          </p>
        </div>
        <Button
          type="button"
          variant="secondary"
          onClick={() => {
            setShowNew(true);
            navigate('/chat');
          }}
        >
          <MessageSquarePlus className="size-4" />
          New chat
        </Button>
      </div>

      <div className="grid gap-6 lg:grid-cols-[240px_minmax(0,1fr)]">
        <aside className="space-y-2" aria-label="Chat sessions">
          {isLoading && (
            <p className="font-body text-sm text-text-muted">Loading sessions…</p>
          )}
          {!isLoading && sessions.length === 0 && (
            <p className="font-body text-sm text-text-muted">No chats yet.</p>
          )}
          {sessions.map((session) => (
            <SessionListItem
              key={session.id}
              session={session}
              agentName={agentNameById.get(session.agentId) ?? 'Agent'}
              active={session.id === sessionId}
            />
          ))}
        </aside>

        <section>
          {showNew && !sessionId ? (
            <NewChatForm
              onCreated={(id) => {
                setShowNew(false);
                navigate(`/chat/${id}`);
              }}
            />
          ) : sessionId ? (
            <ChatSessionPane key={sessionId} sessionId={sessionId} />
          ) : (
            <p className="font-body text-sm text-text-secondary">
              Select a session or start a new chat.
            </p>
          )}
        </section>
      </div>
    </div>
  );
}
