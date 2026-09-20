import { useVirtualizer } from '@tanstack/react-virtual';
import { useEffect, useRef } from 'react';
import { Link } from 'react-router-dom';
import { MarkdownContent } from '../../opencode-session/components/MarkdownContent';
import {
  parseChatActionMetadata,
  type ChatMessage,
} from '../../lib/schemas/chat';
import { cn } from '../../lib/utils';
import { CommentAttachments } from '../tickets/CommentAttachments';
import { useOpenTicket } from '../tickets/useOpenTicket';

const ESTIMATED_ROW_HEIGHT = 72;

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

  if (meta.action === 'cutoff' && meta.childSessionId) {
    return (
      <Link
        to={`/chat/${meta.childSessionId}`}
        className="mt-1.5 inline-flex rounded-md border border-border bg-surface-raised px-2 py-0.5 font-body text-xs text-text-secondary hover:text-text-primary"
        data-testid="chat-action-chip-cutoff"
      >
        Open continued session
      </Link>
    );
  }

  if (meta.action === 'cutoff_seed' && meta.parentSessionId) {
    return (
      <Link
        to={`/chat/${meta.parentSessionId}`}
        className="mt-1.5 inline-flex rounded-md border border-border bg-surface-raised px-2 py-0.5 font-body text-xs text-text-secondary hover:text-text-primary"
        data-testid="chat-action-chip-cutoff-seed"
      >
        View parent session
      </Link>
    );
  }

  return null;
}

export function ChatMessageBubble({ message }: { message: ChatMessage }) {
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
      </article>
    </div>
  );
}

export function ChatMessageList({
  messages,
  thinking,
}: {
  messages: ChatMessage[];
  thinking?: boolean;
}) {
  const parentRef = useRef<HTMLDivElement>(null);

  const virtualizer = useVirtualizer({
    count: messages.length,
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
    if (messages.length === 0) return;
    virtualizer.scrollToIndex(messages.length - 1, { align: 'end' });
  }, [messages.length, virtualizer]);

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
              <ChatMessageBubble message={messages[item.index]!} />
            </div>
          ))}
        </div>
      </div>
      {thinking ? <ThinkingIndicator /> : null}
    </div>
  );
}
