import { useVirtualizer } from '@tanstack/react-virtual';
import { useEffect, useRef, useState, type FormEvent } from 'react';
import { Link } from 'react-router-dom';
import { Button } from '../../components/ui/button';
import { Combobox } from '../../components/ui/combobox';
import { Input } from '../../components/ui/input';
import { Label } from '../../components/ui/label';
import { Textarea } from '../../components/ui/textarea';
import { parseApiErrorMessage } from '../../lib/api';
import { MarkdownContent } from '../../opencode-session/components/MarkdownContent';
import {
  parseChatActionMetadata,
  parseChatProposals,
  type ChatMessage,
  type ChatProposal,
} from '../../lib/schemas/chat';
import { cn } from '../../lib/utils';
import { useBoards } from '../boards/useBoards';
import { CommentAttachments } from '../tickets/CommentAttachments';
import { useOpenTicket } from '../tickets/useOpenTicket';
import { useAcceptChatProposal, useRejectChatProposal } from './useChat';

const ESTIMATED_ROW_HEIGHT = 72;

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
      className="flex max-w-[85%] items-center gap-2 rounded-2xl rounded-bl-md border border-moss-200 bg-moss-50 px-3 py-2 font-body text-sm text-bark-600"
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
  const needsBoard = isTicket || (proposal.scope ?? 'board') !== 'workspace';
  const defaultBoardId = proposal.boardId ?? context.boardId ?? '';
  const [editing, setEditing] = useState(false);
  const [title, setTitle] = useState(proposal.title);
  const [body, setBody] = useState(proposal.description ?? proposal.content ?? '');
  const [boardId, setBoardId] = useState(defaultBoardId);
  const [error, setError] = useState<string | null>(null);
  const busy = accept.isPending || reject.isPending;

  if (proposal.status === 'created') {
    return (
      <section
        className="mt-2 rounded-lg border border-moss-200 bg-paper-50 px-3 py-2 font-body text-xs text-text-secondary"
        data-testid="chat-proposal-card"
        data-status="created"
      >
        <p className="font-medium text-text-primary">{proposal.title}</p>
        {isTicket && proposal.ticketId ? (
          <button
            type="button"
            className="mt-1 inline-flex rounded-md border border-accent/40 bg-accent-muted px-2 py-0.5 text-accent hover:bg-accent/15"
            data-testid="chat-proposal-open-ticket"
            onClick={() => void openTicket(proposal.ticketId!)}
          >
            Created ticket · Open
          </button>
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

  return (
    <form
      className="mt-2 space-y-2 rounded-lg border border-accent/40 bg-paper-50 px-3 py-2 font-body text-xs"
      data-testid="chat-proposal-card"
      data-status="pending"
      onSubmit={(event) => void handleAccept(event)}
    >
      <p className="font-medium text-text-primary">
        {isTicket ? 'Proposed ticket' : 'Proposed knowledge'}
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
            {accept.isPending
              ? 'Saving…'
              : isTicket
                ? 'Create ticket'
                : 'Save knowledge'}
          </Button>
        </div>
      ) : null}
    </form>
  );
}

export function ChatMessageBubble({
  message,
  streaming,
  streamingText,
  proposalContext,
}: {
  message?: ChatMessage;
  streaming?: boolean;
  streamingText?: string;
  proposalContext?: ChatProposalContext;
}) {
  if (streaming) {
    const text = streamingText?.trim() ?? '';
    // The indicator is its own bubble, so it is not wrapped in another one.
    if (!text) {
      return (
        <div className="flex w-full justify-start" data-testid="chat-streaming-bubble">
          <ThinkingIndicator />
        </div>
      );
    }
    return (
      <div className="flex w-full justify-start">
        <article
          data-role="agent"
          aria-label="Agent"
          aria-live="polite"
          aria-busy="true"
          className="max-w-[85%] rounded-2xl rounded-bl-md border border-moss-200 bg-moss-50 px-3 py-1.5 font-body text-sm text-text-primary"
          data-testid="chat-streaming-bubble"
        >
          <div className="leading-snug text-text-primary">
            <MarkdownContent>{text}</MarkdownContent>
          </div>
        </article>
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
        'flex w-full',
        isHuman ? 'justify-end' : isSystem ? 'justify-center' : 'justify-start',
      )}
    >
      <article
        data-role={message.role}
        aria-label={roleLabel(message.role)}
        className={cn(
          'max-w-[85%] px-3 py-1.5 font-body text-sm',
          isHuman &&
            'rounded-2xl rounded-br-md bg-bark-800 text-paper-50 shadow-sm',
          message.role === 'agent' &&
            'rounded-2xl rounded-bl-md border border-moss-200 bg-moss-50 text-text-primary',
          isSystem &&
            'max-w-[92%] rounded-lg border border-info/25 bg-info-muted/60 px-3 py-1.5 text-text-secondary',
        )}
      >
        {isSystem ? (
          <header className="mb-0.5 font-body text-[11px] font-medium uppercase tracking-wide text-info">
            System
          </header>
        ) : null}
        {hasBody ? (
          isHuman ? (
            <p className="whitespace-pre-wrap leading-snug">{message.body}</p>
          ) : (
            <div
              className={cn(
                'leading-snug',
                isSystem ? 'text-text-secondary' : 'text-text-primary',
              )}
            >
              <MarkdownContent>{message.body}</MarkdownContent>
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
    </div>
  );
}

export function ChatMessageList({
  messages,
  thinking,
  streamingAgent,
  proposalContext,
}: {
  messages: ChatMessage[];
  thinking?: boolean;
  streamingAgent?: { runId: string; text: string; error?: string | null };
  proposalContext?: ChatProposalContext;
}) {
  const parentRef = useRef<HTMLDivElement>(null);
  const streamingRow = streamingAgent ? 1 : 0;
  const rowCount = messages.length + streamingRow;

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
  }, [rowCount, virtualizer, streamingAgent?.text]);

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
              className="absolute left-0 top-0 w-full py-1"
              style={{
                transform: `translateY(${item.start}px)`,
              }}
            >
              {item.index < messages.length ? (
                <ChatMessageBubble
                  message={messages[item.index]!}
                  proposalContext={proposalContext}
                />
              ) : (
                <ChatMessageBubble
                  streaming
                  streamingText={
                    streamingAgent?.error
                      ? streamingAgent.error
                      : streamingAgent?.text
                  }
                />
              )}
            </div>
          ))}
        </div>
      </div>
      {thinking && !streamingAgent ? <ThinkingIndicator /> : null}
    </div>
  );
}
