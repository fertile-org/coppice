import { useVirtualizer } from '@tanstack/react-virtual';
import { useEffect, useRef, useState, type FormEvent } from 'react';
import { Link } from 'react-router-dom';
import { Button } from '../../components/ui/button';
import { Combobox } from '../../components/ui/combobox';
import { Input } from '../../components/ui/input';
import { Label } from '../../components/ui/label';
import { Textarea } from '../../components/ui/textarea';
import { parseApiErrorMessage } from '../../lib/api';
import { TicketMarkdown } from '../../components/TicketMarkdown';
import {
  parseChatActionMetadata,
  parseChatProposals,
  type ChatMessage,
  type ChatProposal,
} from '../../lib/schemas/chat';
import type { DesignSet } from '../../lib/schemas/design';
import {
  CollapsibleDetail,
  excerptPreview,
} from '../../opencode-session/components/CollapsibleDetail';
import { MarkdownContent } from '../../opencode-session/components/MarkdownContent';
import { sessionTheme } from '../../opencode-session/theme/session-theme';
import '../../opencode-session/theme/opencode-theme.css';
import { DesignCard } from '../designs/DesignCard';
import { useDesigns, type DesignScope } from '../designs/useDesigns';
import { cn } from '../../lib/utils';
import { useBoards } from '../boards/useBoards';
import { CommentAttachments } from '../tickets/CommentAttachments';
import { useOpenTicket } from '../tickets/useOpenTicket';
import { useAcceptChatProposal, useRejectChatProposal } from './useChat';

export type StreamingAgentState = {
  runId: string;
  thinkingText?: string;
  replyText?: string;
  text?: string;
  error?: string | null;
  streaming?: boolean;
};

const ESTIMATED_ROW_HEIGHT = 72;
const AGENT_PROSE_CLASS =
  '[&_h3]:text-[15px] [&_ol]:text-[15px] [&_p]:text-[15px] [&_ul]:text-[15px]';
const SYSTEM_PROSE_CLASS =
  '[&_li]:text-text-secondary [&_ol]:text-text-secondary [&_p]:text-text-secondary [&_td]:text-text-secondary [&_ul]:text-text-secondary';

export interface ChatProposalContext {
  sessionId: string;
  /** The session's board, if it has one. Ticket proposals need a board to create. */
  boardId: string | null;
  /** Show the proposal state only; no Create, Save, or Reject buttons. */
  readOnly: boolean;
}

export function ThinkingIndicator({
  label = 'Thinking…',
}: {
  label?: string;
}) {
  return (
    <div
      role="status"
      aria-live="polite"
      aria-busy="true"
      className="flex items-center gap-2 font-body text-sm text-text-secondary"
      data-testid="thinking-indicator"
    >
      <span
        className="inline-block size-1.5 animate-pulse rounded-full bg-accent"
        aria-hidden
      />
      {label}
    </div>
  );
}

function roleLabel(role: ChatMessage['role']): string {
  switch (role) {
    case 'human':
      return 'You';
    case 'agent':
      return 'Agent';
    case 'system':
      return 'System';
  }
}

function OpenTicketChip({ ticketId }: { ticketId: string }) {
  const openTicket = useOpenTicket();
  return (
    <button
      type="button"
      className="mt-1.5 inline-flex rounded-md border border-accent/40 bg-accent-muted px-2 py-0.5 font-body text-xs text-accent hover:bg-accent/15"
      data-testid="chat-action-chip-ticket"
      onClick={() => void openTicket(ticketId)}
    >
      Open ticket
    </button>
  );
}

function ActionMetadataChip({ message }: { message: ChatMessage }) {
  const meta = parseChatActionMetadata(message.actionMetadata);
  if (!meta) return null;

  if (meta.action === 'create_ticket' && meta.ticketId) {
    return <OpenTicketChip ticketId={meta.ticketId} />;
  }

  if (meta.action === 'create_knowledge' && meta.knowledgeItemId) {
    return (
      <Link
        to="/knowledge"
        className="mt-1.5 inline-flex rounded-md border border-accent/40 bg-accent-muted px-2 py-0.5 font-body text-xs text-accent hover:bg-accent/15"
        data-testid="chat-action-chip-knowledge"
      >
        View knowledge inbox
      </Link>
    );
  }

  return null;
}

function ChatProposalCard({
  proposal,
  messageId,
  context,
}: {
  proposal: ChatProposal;
  messageId: string;
  context: ChatProposalContext;
}) {
  const openTicket = useOpenTicket();
  const { data: boards = [] } = useBoards();
  const accept = useAcceptChatProposal(context.sessionId);
  const reject = useRejectChatProposal(context.sessionId);
  const isTicket = proposal.type === 'create_ticket';
  const children = proposal.children ?? [];
  const needsBoard = isTicket || (proposal.scope ?? 'board') !== 'workspace';
  const defaultBoardId = proposal.boardId ?? context.boardId ?? '';
  const [editing, setEditing] = useState(false);
  const [title, setTitle] = useState(proposal.title);
  const [body, setBody] = useState(proposal.description ?? proposal.content ?? '');
  const [boardId, setBoardId] = useState(defaultBoardId);
  const [selectedChildIds, setSelectedChildIds] = useState<string[]>(() =>
    children.map((child) => child.id),
  );
  const [error, setError] = useState<string | null>(null);
  const busy = accept.isPending || reject.isPending;
  const selectedCount = selectedChildIds.length;
  const createCount = isTicket ? 1 + selectedCount : 1;

  if (proposal.status === 'created') {
    const childTicketIds = proposal.childTicketIds ?? [];
    return (
      <section
        className="mt-2 rounded-lg border border-moss-200 bg-paper-50 px-3 py-2 font-body text-xs text-text-secondary"
        data-testid="chat-proposal-card"
        data-status="created"
      >
        <p className="font-medium text-text-primary">{proposal.title}</p>
        {isTicket && proposal.ticketId ? (
          <div className="mt-1 flex flex-wrap gap-1">
            <button
              type="button"
              className="inline-flex rounded-md border border-accent/40 bg-accent-muted px-2 py-0.5 text-accent hover:bg-accent/15"
              data-testid="chat-proposal-open-ticket"
              onClick={() => void openTicket(proposal.ticketId!)}
            >
              Created ticket · Open
            </button>
            {childTicketIds.map((id, index) => (
              <button
                key={id}
                type="button"
                className="inline-flex rounded-md border border-accent/40 bg-accent-muted px-2 py-0.5 text-accent hover:bg-accent/15"
                data-testid={`chat-proposal-open-child-${index}`}
                onClick={() => void openTicket(id)}
              >
                Child {index + 1} · Open
              </button>
            ))}
          </div>
        ) : (
          <Link
            to="/knowledge"
            className="mt-1 inline-flex rounded-md border border-accent/40 bg-accent-muted px-2 py-0.5 text-accent hover:bg-accent/15"
            data-testid="chat-proposal-knowledge-link"
          >
            Proposed knowledge · View inbox
          </Link>
        )}
      </section>
    );
  }

  if (proposal.status === 'rejected') {
    return (
      <section
        className="mt-2 rounded-lg border border-border px-3 py-2 font-body text-xs text-text-secondary"
        data-testid="chat-proposal-card"
        data-status="rejected"
      >
        <p className="line-through">{proposal.title}</p>
        <p className="mt-0.5">Rejected</p>
      </section>
    );
  }

  const interactive = !context.readOnly;
  const showBoardPicker = needsBoard && !proposal.boardId && !context.boardId;

  function toggleChild(childId: string, checked: boolean) {
    setSelectedChildIds((current) =>
      checked
        ? current.includes(childId)
          ? current
          : [...current, childId]
        : current.filter((id) => id !== childId),
    );
  }

  async function handleAccept(event?: FormEvent) {
    event?.preventDefault();
    if (needsBoard && !boardId) {
      setError('Select a board.');
      return;
    }
    setError(null);
    try {
      await accept.mutateAsync({
        messageId,
        proposalId: proposal.id,
        title: title.trim() || undefined,
        description: body.trim() || undefined,
        boardId: boardId || undefined,
        selectedChildIds: children.length > 0 ? selectedChildIds : undefined,
      });
    } catch (err) {
      setError(
        parseApiErrorMessage(
          err,
          isTicket ? 'Could not create ticket.' : 'Could not propose knowledge.',
        ),
      );
    }
  }

  async function handleReject() {
    setError(null);
    try {
      await reject.mutateAsync({ messageId, proposalId: proposal.id });
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Could not reject this proposal.'));
    }
  }

  const createLabel = !isTicket
    ? 'Save knowledge'
    : createCount === 1
      ? 'Create ticket'
      : `Create ${createCount} tickets`;

  return (
    <form
      className="mt-2 space-y-2 rounded-lg border border-accent/40 bg-paper-50 px-3 py-2 font-body text-xs"
      data-testid="chat-proposal-card"
      data-status="pending"
      onSubmit={(event) => void handleAccept(event)}
    >
      <p className="font-medium text-text-primary">
        {isTicket
          ? children.length > 0
            ? 'Proposed ticket · with child tickets'
            : 'Proposed ticket'
          : 'Proposed knowledge'}
      </p>
      {editing ? (
        <div className="space-y-2">
          <Label htmlFor={`proposal-title-${proposal.id}`}>Title</Label>
          <Input
            id={`proposal-title-${proposal.id}`}
            aria-label="Title"
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            disabled={busy}
          />
          <Label htmlFor={`proposal-body-${proposal.id}`}>
            {isTicket ? 'Description' : 'Content'}
          </Label>
          <Textarea
            id={`proposal-body-${proposal.id}`}
            aria-label={isTicket ? 'Description' : 'Content'}
            value={body}
            onChange={(event) => setBody(event.target.value)}
            rows={4}
            disabled={busy}
          />
        </div>
      ) : (
        <div>
          <p className="text-sm text-text-primary">{title}</p>
          {body.trim() ? (
            <p className="mt-0.5 line-clamp-3 whitespace-pre-wrap text-text-secondary">
              {body}
            </p>
          ) : null}
        </div>
      )}
      {proposal.designSetId ? (
        <span className="inline-block rounded-full bg-paper-100 px-2 py-0.5 text-[11px] text-text-secondary">
          Design attached
        </span>
      ) : null}
      {children.length > 0 ? (
        <div
          className="space-y-1 border-t border-border pt-2"
          data-testid="chat-proposal-children"
        >
          <p className="font-medium text-text-secondary">
            Child tickets ({selectedCount} of {children.length})
          </p>
          {children.map((child) => {
            const checked = selectedChildIds.includes(child.id);
            return (
              <label
                key={child.id}
                className={cn(
                  'flex gap-2 py-1',
                  !checked && 'text-text-secondary line-through opacity-70',
                )}
              >
                <input
                  type="checkbox"
                  className="mt-0.5"
                  checked={checked}
                  disabled={busy || !interactive}
                  aria-label={child.title}
                  onChange={(event) => toggleChild(child.id, event.target.checked)}
                />
                <span>
                  <span className="font-medium text-text-primary">{child.title}</span>
                  {child.description?.trim() ? (
                    <span className="mt-0.5 block text-text-secondary">
                      {child.description}
                    </span>
                  ) : null}
                </span>
              </label>
            );
          })}
        </div>
      ) : null}
      {interactive && showBoardPicker ? (
        <div className="space-y-1">
          <Label htmlFor={`proposal-board-${proposal.id}`}>Board</Label>
          <Combobox
            id={`proposal-board-${proposal.id}`}
            aria-label="Board"
            value={boardId}
            onValueChange={setBoardId}
            placeholder="Select board"
            searchPlaceholder="Search boards…"
            options={boards.map((board) => ({ value: board.id, label: board.name }))}
          />
        </div>
      ) : null}
      {error ? (
        <p className="text-danger" role="alert">
          {error}
        </p>
      ) : null}
      {interactive ? (
        <div className="flex flex-wrap justify-end gap-2 pt-1">
          <Button
            type="button"
            variant="secondary"
            size="sm"
            disabled={busy}
            onClick={() => setEditing((value) => !value)}
          >
            {editing ? 'Done editing' : 'Edit'}
          </Button>
          <Button
            type="button"
            variant="secondary"
            size="sm"
            disabled={busy}
            onClick={() => void handleReject()}
          >
            Reject
          </Button>
          <Button type="submit" size="sm" disabled={busy || (needsBoard && !boardId)}>
            {accept.isPending ? 'Saving…' : createLabel}
          </Button>
        </div>
      ) : null}
    </form>
  );
}

function designsForRun(
  sets: DesignSet[] | undefined,
  runId: string | null | undefined,
): DesignSet[] | undefined {
  if (!runId || !sets?.length) return undefined;
  const matched = sets.filter((set) => set.runId === runId);
  return matched.length > 0 ? matched : undefined;
}

function StreamingThinking({
  text,
  streaming,
}: {
  text: string;
  streaming: boolean;
}) {
  const content = text.replaceAll('[REDACTED]', '').trim();
  if (!content) return null;
  return (
    <div className="oc-session" data-testid="chat-streaming-thinking">
      <CollapsibleDetail
        label="Thinking"
        preview={excerptPreview(content)}
        streaming={streaming}
      >
        <div className={sessionTheme.fontMonoSm}>
          <MarkdownContent tone="thinking">{content}</MarkdownContent>
        </div>
      </CollapsibleDetail>
    </div>
  );
}

export function ChatMessageBubble({
  message,
  streaming,
  streamingText,
  streamingThinking,
  streamingReply,
  streamingLive = true,
  proposalContext,
  designs,
  designScope,
}: {
  message?: ChatMessage;
  streaming?: boolean;
  streamingText?: string;
  streamingThinking?: string;
  streamingReply?: string;
  streamingLive?: boolean;
  proposalContext?: ChatProposalContext;
  designs?: DesignSet[];
  designScope?: DesignScope;
}) {
  if (streaming) {
    const thinking = (streamingThinking ?? '').trim();
    const reply = (streamingReply ?? streamingText ?? '').trim();
    if (!thinking && !reply) {
      return (
        <div className="flex w-full justify-start" data-testid="chat-streaming-bubble">
          <ThinkingIndicator />
        </div>
      );
    }
    return (
      <div
        className="flex w-full flex-col gap-3"
        data-testid="chat-streaming-bubble"
      >
        {thinking ? (
          <StreamingThinking text={thinking} streaming={streamingLive} />
        ) : null}
        {reply ? (
          <article
            data-role="agent"
            aria-label="Agent"
            aria-live="polite"
            aria-busy={streamingLive}
            className="w-full min-w-0 font-body text-text-primary"
            data-testid="chat-streaming-reply"
          >
            <div className={AGENT_PROSE_CLASS}>
              <TicketMarkdown mermaidAsCode>{reply}</TicketMarkdown>
            </div>
          </article>
        ) : null}
      </div>
    );
  }

  if (!message) return null;

  const isHuman = message.role === 'human';
  const isSystem = message.role === 'system';
  const attachments =
    message.attachments.length > 0
      ? message.attachments
      : message.attachmentIds.map((id) => ({
          id,
          filename: 'Attachment',
          contentType: 'application/octet-stream',
          sizeBytes: 0,
        }));
  const hasBody = message.body.trim().length > 0;

  return (
    <div
      className={cn(
        'flex w-full flex-col gap-2',
        isHuman ? 'items-end justify-end' : 'items-start justify-start',
      )}
    >
      <article
        data-role={message.role}
        aria-label={roleLabel(message.role)}
        className={cn(
          'font-body text-sm',
          isHuman &&
            'max-w-[85%] rounded-2xl rounded-br-md bg-paper-200 px-3 py-1.5 text-text-primary shadow-sm',
          message.role === 'agent' && 'w-full min-w-0 text-text-primary',
          isSystem &&
            'w-full min-w-0 border-l-2 border-info pl-3 text-text-secondary',
        )}
      >
        {isSystem ? (
          <header className="mb-0.5 font-body text-[11px] font-medium uppercase tracking-wide text-info">
            System
          </header>
        ) : null}
        {hasBody ? (
          isHuman ? (
            <p className="whitespace-pre-wrap leading-relaxed">{message.body}</p>
          ) : (
            <div className={isSystem ? SYSTEM_PROSE_CLASS : AGENT_PROSE_CLASS}>
              <TicketMarkdown>{message.body}</TicketMarkdown>
            </div>
          )
        ) : null}
        <CommentAttachments attachments={attachments} />
        <ActionMetadataChip message={message} />
        {proposalContext && message.role === 'agent'
          ? parseChatProposals(message.actionMetadata).map((proposal) => (
              <ChatProposalCard
                key={proposal.id}
                proposal={proposal}
                messageId={message.id}
                context={proposalContext}
              />
            ))
          : null}
      </article>
      {message.role === 'agent'
        ? designs?.map((set) => (
            <div key={set.id} className="w-full">
              <DesignCard design={set} scope={designScope} />
            </div>
          ))
        : null}
    </div>
  );
}

export function ChatMessageList({
  messages,
  thinking,
  streamingAgent,
  proposalContext,
  chatSessionId,
}: {
  messages: ChatMessage[];
  thinking?: boolean;
  streamingAgent?: StreamingAgentState;
  proposalContext?: ChatProposalContext;
  chatSessionId?: string;
}) {
  const { data: designSets } = useDesigns(
    chatSessionId ? { chatSessionId } : undefined,
  );
  const parentRef = useRef<HTMLDivElement>(null);
  const streamingRow = streamingAgent ? 1 : 0;
  const rowCount = messages.length + streamingRow;
  const streamScrollKey = [
    streamingAgent?.thinkingText,
    streamingAgent?.replyText,
    streamingAgent?.text,
    streamingAgent?.error,
  ].join('\0');

  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ESTIMATED_ROW_HEIGHT,
    measureElement: (element) => {
      const measured = element.getBoundingClientRect().height;
      // jsdom / pre-layout often reports 0; keep the estimate until real layout exists.
      return measured > 0 ? measured : ESTIMATED_ROW_HEIGHT;
    },
    overscan: 8,
    initialRect: { width: 640, height: 480 },
  });

  useEffect(() => {
    if (rowCount === 0) return;
    virtualizer.scrollToIndex(rowCount - 1, { align: 'end' });
  }, [rowCount, virtualizer, streamScrollKey]);

  return (
    <div className="flex h-full min-h-0 flex-col gap-1.5">
      <div
        ref={parentRef}
        role="log"
        aria-label="Chat transcript"
        data-testid="chat-message-list"
        className="min-h-0 flex-1 overflow-y-auto px-1"
      >
        <div
          className="relative w-full"
          style={{ height: `${virtualizer.getTotalSize()}px` }}
        >
          {virtualizer.getVirtualItems().map((item) => (
            <div
              key={item.key}
              data-index={item.index}
              ref={virtualizer.measureElement}
              className="absolute left-0 top-0 w-full py-3"
              style={{
                transform: `translateY(${item.start}px)`,
              }}
            >
              <div className="mx-auto w-full max-w-[720px]">
                {item.index < messages.length ? (
                  <ChatMessageBubble
                    message={messages[item.index]!}
                    proposalContext={proposalContext}
                    designs={designsForRun(designSets, messages[item.index]?.agentRunId)}
                    designScope={chatSessionId ? { chatSessionId } : undefined}
                  />
                ) : (
                  <ChatMessageBubble
                    streaming
                    streamingThinking={
                      streamingAgent?.error
                        ? undefined
                        : streamingAgent?.thinkingText
                    }
                    streamingReply={
                      streamingAgent?.error
                        ? streamingAgent.error
                        : (streamingAgent?.replyText ?? streamingAgent?.text)
                    }
                    streamingLive={streamingAgent?.streaming !== false}
                  />
                )}
              </div>
            </div>
          ))}
        </div>
      </div>
      {thinking && !streamingAgent ? (
        <div className="mx-auto w-full max-w-[720px]">
          <ThinkingIndicator />
        </div>
      ) : null}
    </div>
  );
}
