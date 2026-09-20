import { MessageSquarePlus } from 'lucide-react';
import { useCallback, useMemo, useState, type FormEvent } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { Button } from '../../components/ui/button';
import { Label } from '../../components/ui/label';
import { Textarea } from '../../components/ui/textarea';
import { parseApiErrorMessage } from '../../lib/api';
import type { ChatSession } from '../../lib/schemas/chat';
import { useAgents } from '../agents/useAgents';
import { useProjects } from '../projects/useProjects';
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
  const [body, setBody] = useState('');
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    const trimmed = body.trim();
    if (!trimmed || disabled) return;
    setError(null);
    try {
      const result = await postMessage.mutateAsync(trimmed);
      setBody('');
      onPosted(result.runId);
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Could not send message.'));
    }
  }

  return (
    <form
      onSubmit={(event) => void handleSubmit(event)}
      className="space-y-2 border-t border-border pt-3"
      data-testid="chat-composer"
    >
      <Label htmlFor="chat-composer-input" className="sr-only">
        Message
      </Label>
      <Textarea
        id="chat-composer-input"
        value={body}
        onChange={(event) => setBody(event.target.value)}
        placeholder="Message the agent…"
        rows={3}
        disabled={disabled || postMessage.isPending}
      />
      {error && (
        <p className="font-body text-sm text-danger" role="alert">
          {error}
        </p>
      )}
      <div className="flex justify-end">
        <Button
          type="submit"
          disabled={disabled || postMessage.isPending || !body.trim()}
        >
          {postMessage.isPending ? 'Sending…' : 'Send'}
        </Button>
      </div>
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
    <div className="flex h-[min(70vh,720px)] flex-col gap-3">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="font-display text-lg font-semibold text-text-primary">
            {agentName}
          </h2>
          <p className="font-body text-xs text-text-secondary">
            {session ? `${session.status} · ${formatSessionTime(session.updatedAt)}` : 'Loading…'}
          </p>
        </div>
        {session && (
          <ChatSessionActions session={session} disabled={awaiting} />
        )}
      </header>

      <div className="min-h-0 flex-1 rounded-lg border border-border bg-surface-raised p-2">
        <ChatMessageList messages={messages} thinking={awaiting && !activeRunId} />
      </div>

      {activeRunId && (
        <ChatLiveTurn runId={activeRunId} onFinished={onLiveFinished} />
      )}

      {cutoff ? (
        <p className="font-body text-sm text-text-secondary">
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
