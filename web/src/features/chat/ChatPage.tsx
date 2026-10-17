import { useQueryClient } from '@tanstack/react-query';
import { Paperclip, X } from 'lucide-react';
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent,
} from 'react';
import { Link, useLocation, useNavigate, useParams } from 'react-router-dom';
import { Button } from '../../components/ui/button';
import { Label } from '../../components/ui/label';
import { Textarea } from '../../components/ui/textarea';
import { parseApiErrorMessage } from '../../lib/api';
import {
  formatFileSize,
  isImageContentType,
} from '../../lib/attachments';
import type { ChatMessage, ChatSession } from '../../lib/schemas/chat';
import { chatIsolationBanner, chatUnavailableMessage } from '../tools/connectorCopy';
import { cn } from '../../lib/utils';
import { useAgents, type Agent } from '../agents/useAgents';
import { useUploadAttachment } from '../tickets/useTicket';
import { ChatMessageList } from './ChatMessageList';
import { ChatSessionActions } from './ChatSessionActions';
import {
  CHAT_SESSIONS_QUERY_KEY,
  fetchOlderMessages,
  useChatMessages,
  useChatSession,
  useChatSessions,
  useCreateChatSession,
  usePostChatMessage,
} from './useChat';
import { useChatRunLiveStream } from './useChatRunLiveStream';

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

function chatLabel(session: ChatSession): string {
  const title = session.title?.trim();
  return title ? title : 'New chat';
}

function sessionsForAgent(sessions: ChatSession[], agentId: string): ChatSession[] {
  return sessions
    .filter((session) => session.agentId === agentId)
    .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
}

function ChatComposer({
  sessionId,
  agentId,
  disabled,
  onPosted,
}: {
  sessionId: string | null;
  agentId: string;
  disabled?: boolean;
  onPosted: (runId: string, sessionId: string) => void;
}) {
  const postMessage = usePostChatMessage(sessionId ?? '');
  const createSession = useCreateChatSession();
  const uploadAttachment = useUploadAttachment();
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [body, setBody] = useState('');
  const [pendingFiles, setPendingFiles] = useState<PendingFile[]>([]);
  const [error, setError] = useState<string | null>(null);
  const composerLocked = Boolean(
    disabled ||
      postMessage.isPending ||
      uploadAttachment.isPending ||
      createSession.isPending,
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
      let targetId = sessionId;
      const createdNow = !targetId;
      if (!targetId) {
        const created = await createSession.mutateAsync({ agentId });
        targetId = created.id;
      }
      const result = await postMessage.mutateAsync({
        body: trimmed,
        attachmentIds: attachmentIds.length > 0 ? attachmentIds : undefined,
        ...(createdNow ? { sessionId: targetId } : {}),
      });
      setBody('');
      clearPendingFiles();
      onPosted(result.runId, targetId);
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
          {postMessage.isPending ||
          uploadAttachment.isPending ||
          createSession.isPending
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

function ChatTranscript({
  sessionId,
  agent,
  initialRunId,
  onPosted,
}: {
  sessionId: string | null;
  agent: Agent;
  initialRunId?: string | null;
  onPosted: (runId: string, sessionId: string) => void;
}) {
  const { data: session } = useChatSession(sessionId);
  const [activeRunId, setActiveRunId] = useState<string | null>(initialRunId ?? null);
  const awaiting = Boolean(activeRunId) || Boolean(session?.hasActiveRun);

  // Poll the transcript only while a reply is pending and its live stream is
  // not connected; an open stream already shows the reply as it arrives.
  const pollRef = useRef({ awaiting, liveOpen: false });
  pollRef.current.awaiting = awaiting;
  const {
    data: page,
    refetch,
    isLoading: messagesLoading,
    isFetched,
  } = useChatMessages(sessionId, {
    refetchInterval: () =>
      pollRef.current.awaiting && !pollRef.current.liveOpen ? 1500 : false,
  });
  const [older, setOlder] = useState<{
    sessionId: string | null;
    messages: ChatMessage[];
    beforeSeq: number | null;
  }>({ sessionId: null, messages: [], beforeSeq: null });
  const [loadingOlder, setLoadingOlder] = useState(false);
  const olderForSession = older.sessionId === sessionId ? older : null;
  const messages = useMemo(() => {
    const newest = page?.messages ?? [];
    const earlier = olderForSession?.messages ?? [];
    if (earlier.length === 0) return newest;
    const ids = new Set(newest.map((message) => message.id));
    return [...earlier.filter((message) => !ids.has(message.id)), ...newest];
  }, [page?.messages, olderForSession?.messages]);
  const nextOlderSeq = olderForSession
    ? olderForSession.beforeSeq
    : (page?.olderBeforeSeq ?? null);

  async function loadOlder() {
    if (!sessionId || nextOlderSeq === null) return;
    setLoadingOlder(true);
    try {
      const earlier = await fetchOlderMessages(sessionId, nextOlderSeq);
      setOlder({
        sessionId,
        messages: [...earlier.messages, ...(olderForSession?.messages ?? [])],
        beforeSeq: earlier.olderBeforeSeq ?? null,
      });
    } finally {
      setLoadingOlder(false);
    }
  }

  useEffect(() => {
    if (activeRunId) return;
    if (session?.hasActiveRun && session.activeRunId) {
      setActiveRunId(session.activeRunId);
    }
  }, [activeRunId, session?.hasActiveRun, session?.activeRunId]);

  const chatBanner = chatIsolationBanner(agent.connector);
  const chatUnavailable = chatUnavailableMessage(agent.connector);
  const queryClient = useQueryClient();

  const onLiveFinished = useCallback(() => {
    setActiveRunId(null);
    void refetch();
    void queryClient.invalidateQueries({ queryKey: CHAT_SESSIONS_QUERY_KEY });
  }, [queryClient, refetch]);

  const live = useChatRunLiveStream(activeRunId, { onFinished: onLiveFinished });
  pollRef.current.liveOpen = live.connection === 'open';
  const streamingAgent = activeRunId
    ? { runId: activeRunId, text: live.text, error: live.error }
    : undefined;

  const readOnly = session?.status === 'archived' || session?.status === 'cutoff';
  const showEmpty =
    !chatUnavailable &&
    !readOnly &&
    !awaiting &&
    messages.length === 0 &&
    (!sessionId || (isFetched && !messagesLoading));

  return (
    <div
      className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl border border-border bg-surface-raised shadow-sm"
      data-testid="chat-session-pane"
    >
      <header
        className="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-border bg-paper-50/80 px-4 py-3"
        data-testid="chat-session-header"
      >
        <div className="min-w-0">
          <h2 className="font-display text-base font-semibold text-text-primary">
            {agent.name}
          </h2>
          {agent.role ? (
            <p className="mt-0.5 font-body text-xs text-text-secondary">{agent.role}</p>
          ) : null}
        </div>
        {session && session.status === 'active' ? (
          <ChatSessionActions session={session} disabled={awaiting} />
        ) : null}
      </header>

      <div className="flex min-h-0 flex-1 flex-col gap-2 bg-paper-100/40 px-3 py-2">
        {showEmpty ? (
          <p
            className="m-auto max-w-sm px-4 text-center font-body text-sm text-text-secondary"
            data-testid="chat-empty-state"
          >
            Say hi to {agent.name}. They can see all your projects.
          </p>
        ) : (
          <>
          {nextOlderSeq !== null && (
            <Button
              type="button"
              variant="secondary"
              onClick={() => void loadOlder()}
              disabled={loadingOlder}
              className="self-center"
            >
              {loadingOlder ? 'Loading…' : 'Load earlier messages'}
            </Button>
          )}
          <ChatMessageList
            messages={messages}
            thinking={awaiting && !activeRunId}
            streamingAgent={streamingAgent}
          />
          </>
        )}
      </div>

      {chatBanner ? (
        <p
          className="shrink-0 border-t border-warning-muted bg-warning-muted/40 px-4 py-3 font-body text-sm text-text-primary"
          role="status"
          data-testid="chat-isolation-banner"
        >
          {chatBanner}
        </p>
      ) : null}
      {session?.status === 'archived' ? (
        <p className="shrink-0 border-t border-border bg-paper-50/90 px-4 py-3 font-body text-sm text-text-secondary">
          This session is archived. History stays readable.
        </p>
      ) : chatUnavailable ? (
        <p
          className="shrink-0 border-t border-warning-muted bg-warning-muted/40 px-4 py-3 font-body text-sm text-text-primary"
          role="status"
        >
          {chatUnavailable}
        </p>
      ) : (
        <ChatComposer
          sessionId={sessionId}
          agentId={agent.id}
          disabled={awaiting}
          onPosted={(runId, nextSessionId) => {
            if (sessionId && nextSessionId === sessionId) {
              setActiveRunId(runId);
            }
            onPosted(runId, nextSessionId);
          }}
        />
      )}
    </div>
  );
}

function AgentRow({
  agent,
  sessions,
  drafting,
  openSessionId,
}: {
  agent: Agent;
  sessions: ChatSession[];
  drafting: boolean;
  openSessionId?: string;
}) {
  const navigate = useNavigate();
  const unavailable = chatUnavailableMessage(agent.connector);
  const latest = sessions[0];
  const expanded = sessions.length > 1 || (drafting && sessions.length > 0);
  const active =
    drafting ||
    sessions.some((session) => session.id === openSessionId);
  const href = latest ? `/chat/${latest.id}` : `/chat/new/${agent.id}`;
  const replying = sessions.some((session) => session.hasActiveRun);

  if (unavailable) {
    return (
      <div
        className="rounded-md px-3 py-2 opacity-60"
        data-testid="chat-agent-unavailable"
        aria-disabled="true"
      >
        <div className="font-body text-sm font-medium text-text-muted">{agent.name}</div>
        {agent.role ? (
          <div className="mt-0.5 font-body text-xs text-text-muted">{agent.role}</div>
        ) : null}
        <p
          className="mt-1 font-body text-xs text-text-muted"
          data-testid="chat-unavailable"
        >
          {unavailable}
        </p>
      </div>
    );
  }

  return (
    <div data-testid="chat-agent-row">
      <div className="flex items-start gap-1">
        <Link
          to={href}
          className={cn(
            'min-w-0 flex-1 rounded-md px-3 py-2 transition-colors duration-fast',
            active
              ? 'bg-accent-muted text-accent'
              : 'hover:bg-paper-100',
          )}
          aria-current={active ? 'page' : undefined}
        >
          <div className="flex items-center gap-1.5">
            <span className="truncate font-body text-sm font-medium text-text-primary">
              {agent.name}
            </span>
            {replying ? (
              <span className="shrink-0 font-body text-xs text-moss-700" data-testid="chat-session-active">
                Replying…
              </span>
            ) : null}
          </div>
          {agent.role ? (
            <div className="mt-0.5 truncate font-body text-xs text-text-secondary">
              {agent.role}
            </div>
          ) : null}
          {!expanded ? (
            <p className="mt-0.5 truncate font-body text-xs text-text-muted">
              {sessions.length === 0
                ? drafting
                  ? 'New chat'
                  : 'No messages yet'
                : chatLabel(sessions[0])}
            </p>
          ) : null}
        </Link>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="mt-1 shrink-0"
          title={`New chat with ${agent.name}`}
          aria-label={`New chat with ${agent.name}`}
          onClick={() => navigate(`/chat/new/${agent.id}`)}
        >
          New chat
        </Button>
      </div>
      {expanded ? (
        <ul className="mb-1 ml-3 space-y-0.5 border-l border-border pl-2">
          {drafting ? (
            <li>
              <Link
                to={`/chat/new/${agent.id}`}
                className="block truncate rounded-md bg-accent-muted px-2 py-1 font-body text-xs text-accent"
                aria-current="page"
              >
                New chat
              </Link>
            </li>
          ) : null}
          {sessions.map((session) => (
            <li key={session.id}>
              <Link
                to={`/chat/${session.id}`}
                className={cn(
                  'block truncate rounded-md px-2 py-1 font-body text-xs',
                  session.id === openSessionId
                    ? 'bg-accent-muted text-accent'
                    : 'text-text-secondary hover:bg-paper-100 hover:text-text-primary',
                )}
                aria-current={session.id === openSessionId ? 'page' : undefined}
              >
                {chatLabel(session)}
              </Link>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

export function ChatPage() {
  const { sessionId, agentId: draftAgentId } = useParams<{
    sessionId?: string;
    agentId?: string;
  }>();
  const navigate = useNavigate();
  const location = useLocation();
  const { data: agents = [] } = useAgents();
  const { data: sessions = [], isLoading } = useChatSessions();
  const seededRunId =
    (location.state as { runId?: string } | null)?.runId ?? null;

  const enabledAgents = useMemo(
    () =>
      agents
        .filter((agent) => agent.enabled)
        .sort((a, b) => a.name.localeCompare(b.name)),
    [agents],
  );

  const openSession = sessions.find((session) => session.id === sessionId);
  const selectedAgentId = draftAgentId ?? openSession?.agentId;
  const selectedAgent = agents.find((agent) => agent.id === selectedAgentId);

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4" data-testid="chat-page">
      <h1 className="shrink-0 font-display text-2xl font-semibold text-text-primary">
        Chat
      </h1>

      <div
        className="grid min-h-0 flex-1 gap-4 overflow-hidden lg:grid-cols-[280px_minmax(0,1fr)]"
        data-testid="chat-layout-grid"
      >
        <aside
          className="flex min-h-0 flex-col gap-2 overflow-hidden lg:h-full lg:max-h-full lg:self-stretch"
          aria-label="Agents"
          data-testid="chat-session-list"
        >
          <div className="min-h-0 flex-1 space-y-1 overflow-y-auto">
            {isLoading ? (
              <p className="font-body text-sm text-text-muted">Loading chats…</p>
            ) : null}
            {enabledAgents.map((agent) => (
              <AgentRow
                key={agent.id}
                agent={agent}
                sessions={sessionsForAgent(sessions, agent.id)}
                drafting={draftAgentId === agent.id}
                openSessionId={sessionId}
              />
            ))}
          </div>
        </aside>

        <section className="flex min-h-0 min-w-0 flex-col overflow-hidden lg:h-full">
          {selectedAgent ? (
            <ChatTranscript
              key={sessionId ?? `new-${selectedAgent.id}`}
              sessionId={sessionId ?? null}
              agent={selectedAgent}
              initialRunId={seededRunId}
              onPosted={(runId, nextSessionId) => {
                if (!sessionId || nextSessionId !== sessionId) {
                  navigate(`/chat/${nextSessionId}`, { state: { runId } });
                }
              }}
            />
          ) : null}
        </section>
      </div>
    </div>
  );
}
