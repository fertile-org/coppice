import { useEffect, useMemo, useRef, useState } from 'react';
import { X } from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import { formatFileSize, isImageContentType } from '../../lib/attachments';
import { Button } from '../../components/ui/button';
import { CommentAttachments } from './CommentAttachments';
import { useAgents } from '../agents/useAgents';
import {
  useComments,
  useCreateComment,
  useUploadAttachment,
  type Comment,
} from './useTicket';

interface TicketCommentsTabProps {
  ticketId: string;
}

interface PendingFile {
  key: string;
  file: File;
  previewUrl: string | null;
}

function authorLabel(
  comment: Comment,
  agentNamesById: ReadonlyMap<string, string>,
): string {
  switch (comment.authorType) {
    case 'agent':
      if (comment.authorId) {
        const name = agentNamesById.get(comment.authorId);
        if (name) return name;
      }
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

function pendingFileFromFile(file: File): PendingFile {
  return {
    key: `${file.name}-${file.size}-${file.lastModified}-${crypto.randomUUID()}`,
    file,
    previewUrl: isImageContentType(file.type)
      ? URL.createObjectURL(file)
      : null,
  };
}

export function TicketCommentsTab({ ticketId }: TicketCommentsTabProps) {
  const { data: comments, isLoading, isError } = useComments(ticketId);
  const { data: agents } = useAgents();
  const agentNamesById = useMemo(
    () => new Map((agents ?? []).map((agent) => [agent.id, agent.name])),
    [agents],
  );
  const createComment = useCreateComment(ticketId);
  const uploadAttachment = useUploadAttachment();
  const fileInputRef = useRef<HTMLInputElement>(null);

  const [body, setBody] = useState('');
  const [pendingFiles, setPendingFiles] = useState<PendingFile[]>([]);
  const [error, setError] = useState<string | null>(null);

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
    setPendingFiles((current) => [...current, ...next]);
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

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const trimmed = body.trim();
    if (!trimmed) return;

    setError(null);
    try {
      const attachmentIds: string[] = [];
      for (const pending of pendingFiles) {
        const uploaded = await uploadAttachment.mutateAsync(pending.file);
        attachmentIds.push(uploaded.id);
      }

      await createComment.mutateAsync({
        body: trimmed,
        attachmentIds: attachmentIds.length > 0 ? attachmentIds : undefined,
      });

      setBody('');
      setPendingFiles((current) => {
        for (const pending of current) {
          if (pending.previewUrl) {
            URL.revokeObjectURL(pending.previewUrl);
          }
        }
        return [];
      });
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
                {authorLabel(comment, agentNamesById)}
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
            <CommentAttachments
              attachments={
                comment.attachments.length > 0
                  ? comment.attachments
                  : comment.attachmentIds.map((id) => ({
                      id,
                      filename: 'Attachment',
                      contentType: 'application/octet-stream',
                      sizeBytes: 0,
                    }))
              }
            />
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
          className="field-control w-full px-3 py-2 font-body text-sm"
        />

        {pendingFiles.length > 0 && (
          <ul className="flex flex-wrap gap-2">
            {pendingFiles.map((pending) => (
              <li key={pending.key} className="relative">
                {pending.previewUrl ? (
                  <img
                    src={pending.previewUrl}
                    alt={pending.file.name}
                    className="size-16 rounded-md border border-border object-cover"
                  />
                ) : (
                  <div className="flex size-16 flex-col items-center justify-center rounded-md border border-border bg-surface px-1 text-center">
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
                >
                  <X className="size-3" />
                </button>
              </li>
            ))}
          </ul>
        )}

        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-2">
            <input
              ref={fileInputRef}
              type="file"
              multiple
              onChange={(e) => {
                if (e.target.files) {
                  addPendingFiles(e.target.files);
                }
                e.target.value = '';
              }}
              className="max-w-xs font-body text-xs text-text-secondary file:mr-2 file:rounded-md file:border file:border-border file:bg-surface-raised file:px-2 file:py-1 file:font-body file:text-xs file:text-text-secondary"
            />
            {pendingFiles.length > 0 && (
              <span className="font-body text-xs text-text-muted">
                {pendingFiles.length} file{pendingFiles.length === 1 ? '' : 's'}{' '}
                selected
              </span>
            )}
          </div>

          <Button type="submit" disabled={isBusy || body.trim().length === 0}>
            {isBusy ? 'Posting…' : 'Post comment'}
          </Button>
        </div>
      </form>
    </div>
  );
}
