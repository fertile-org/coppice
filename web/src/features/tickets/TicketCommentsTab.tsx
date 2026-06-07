import { useRef, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import {
  useComments,
  useCreateComment,
  useUploadAttachment,
  type Comment,
} from './useTicket';

interface TicketCommentsTabProps {
  ticketId: string;
}

function authorLabel(comment: Comment): string {
  switch (comment.authorType) {
    case 'agent':
      return 'Agent';
    case 'system':
      return 'System';
    default:
      return 'You';
  }
}

function formatTime(iso: string): string {
  try {
    return new Intl.DateTimeFormat(undefined, {
      dateStyle: 'medium',
      timeStyle: 'short',
    }).format(new Date(iso));
  } catch {
    return iso;
  }
}

export function TicketCommentsTab({ ticketId }: TicketCommentsTabProps) {
  const { data: comments, isLoading, isError } = useComments(ticketId);
  const createComment = useCreateComment(ticketId);
  const uploadAttachment = useUploadAttachment();
  const fileInputRef = useRef<HTMLInputElement>(null);

  const [body, setBody] = useState('');
  const [pendingFile, setPendingFile] = useState<File | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const trimmed = body.trim();
    if (!trimmed) return;

    setError(null);
    try {
      const attachmentIds: string[] = [];
      if (pendingFile) {
        const uploaded = await uploadAttachment.mutateAsync(pendingFile);
        attachmentIds.push(uploaded.id);
      }

      await createComment.mutateAsync({
        body: trimmed,
        attachmentIds: attachmentIds.length > 0 ? attachmentIds : undefined,
      });

      setBody('');
      setPendingFile(null);
      if (fileInputRef.current) fileInputRef.current.value = '';
    } catch {
      setError('Unable to post comment.');
    }
  }

  const isBusy = createComment.isPending || uploadAttachment.isPending;

  return (
    <div className="flex h-full flex-col gap-4">
      <div className="min-h-0 flex-1 space-y-3 overflow-y-auto">
        {isLoading && (
          <p className="font-body text-sm text-text-muted">Loading comments…</p>
        )}

        {isError && (
          <p className="font-body text-sm text-danger">Unable to load comments.</p>
        )}

        {!isLoading && !isError && (comments?.length ?? 0) === 0 && (
          <p className="font-body text-sm text-text-muted">
            No comments yet. Start the thread below.
          </p>
        )}

        {comments?.map((comment) => (
          <article
            key={comment.id}
            className="rounded-md border border-border bg-surface px-4 py-3"
          >
            <header className="mb-2 flex items-center justify-between gap-2">
              <span className="font-body text-xs font-medium uppercase tracking-wide text-text-secondary">
                {authorLabel(comment)}
              </span>
              <time
                dateTime={comment.createdAt}
                className="font-body text-xs text-text-muted"
              >
                {formatTime(comment.createdAt)}
              </time>
            </header>
            <div className="font-body text-sm text-text-primary [&_a]:text-accent [&_code]:rounded [&_code]:bg-paper-200 [&_code]:px-1 [&_p+p]:mt-2">
              <ReactMarkdown>{comment.body}</ReactMarkdown>
            </div>
            {comment.attachmentIds.length > 0 && (
              <ul className="mt-3 flex flex-wrap gap-2">
                {comment.attachmentIds.map((id) => (
                  <li key={id}>
                    <a
                      href={`/api/attachments/${id}`}
                      className="inline-flex items-center rounded-full border border-border bg-paper-200 px-2.5 py-0.5 font-body text-xs text-accent hover:underline"
                    >
                      Attachment
                    </a>
                  </li>
                ))}
              </ul>
            )}
          </article>
        ))}
      </div>

      <form
        onSubmit={(e) => void handleSubmit(e)}
        className="shrink-0 space-y-3 border-t border-border pt-4"
      >
        {error && (
          <p className="rounded-md border border-danger-muted bg-danger-muted/40 px-3 py-2 font-body text-sm text-danger">
            {error}
          </p>
        )}

        <textarea
          value={body}
          onChange={(e) => setBody(e.target.value)}
          rows={4}
          placeholder="Write a comment in markdown…"
          className="w-full rounded-md border border-border bg-surface px-3 py-2 font-body text-sm text-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-muted"
        />

        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-2">
            <input
              ref={fileInputRef}
              type="file"
              onChange={(e) => setPendingFile(e.target.files?.[0] ?? null)}
              className="max-w-xs font-body text-xs text-text-secondary file:mr-2 file:rounded-md file:border file:border-border file:bg-surface-raised file:px-2 file:py-1 file:font-body file:text-xs file:text-text-secondary"
            />
            {pendingFile && (
              <span className="font-body text-xs text-text-muted">
                {pendingFile.name}
              </span>
            )}
          </div>

          <button
            type="submit"
            disabled={isBusy || body.trim().length === 0}
            className="rounded-md bg-accent px-4 py-2 font-body text-sm font-medium text-white transition-colors duration-fast hover:bg-accent-hover disabled:opacity-50"
          >
            {isBusy ? 'Posting…' : 'Post comment'}
          </button>
        </div>
      </form>
    </div>
  );
}
