import { useVirtualizer } from '@tanstack/react-virtual';
import { useEffect, useRef } from 'react';
import { Link } from 'react-router-dom';
import { MarkdownContent } from '../../opencode-session/components/MarkdownContent';
import {
  parseChatActionMetadata,
  type ChatMessage,
} from '../../lib/schemas/chat';
import { cn } from '../../lib/utils';
import { useOpenTicket } from '../tickets/useOpenTicket';

const ESTIMATED_ROW_HEIGHT = 88;

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
        className="inline-block size-2 animate-pulse rounded-full bg-accent"
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
      className="mt-2 inline-flex rounded-md border border-accent/40 bg-accent-muted px-2 py-0.5 font-body text-xs text-accent hover:bg-accent/15"
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
        className="mt-2 inline-flex rounded-md border border-accent/40 bg-accent-muted px-2 py-0.5 font-body text-xs text-accent hover:bg-accent/15"
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
        className="mt-2 inline-flex rounded-md border border-border bg-surface-raised px-2 py-0.5 font-body text-xs text-text-secondary hover:text-text-primary"
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
        className="mt-2 inline-flex rounded-md border border-border bg-surface-raised px-2 py-0.5 font-body text-xs text-text-secondary hover:text-text-primary"
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
  return (
    <article
      data-role={message.role}
      className={cn(
        'rounded-lg border px-3 py-2',
        isHuman
          ? 'ml-8 border-border bg-surface-raised'
          : 'mr-8 border-border bg-paper-100',
      )}
    >
      <header className="mb-1 font-body text-xs font-medium text-text-secondary">
        {roleLabel(message.role)}
      </header>
      {isHuman ? (
        <p className="whitespace-pre-wrap font-body text-sm text-text-primary">
          {message.body}
        </p>
      ) : (
        <div className="font-body text-sm text-text-primary">
          <MarkdownContent>{message.body}</MarkdownContent>
        </div>
      )}
      <ActionMetadataChip message={message} />
    </article>
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
    <div className="flex h-full min-h-[280px] flex-col gap-2">
      <div
        ref={parentRef}
        role="log"
        aria-label="Chat transcript"
        data-testid="chat-message-list"
        className="min-h-0 flex-1 overflow-y-auto"
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
              className="absolute left-0 top-0 w-full px-1 py-1.5"
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
