import { useEffect, useMemo, useRef, useState } from 'react';
import { X } from 'lucide-react';
import { TicketMarkdown } from '../../components/TicketMarkdown';
import { useToast } from '../../components/ToastProvider';
import { formatFileSize, isImageContentType } from '../../lib/attachments';
import type { MentionMode } from '../../lib/schemas/ticket';
import { Button } from '../../components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../../components/ui/select';
import { CommentAttachments } from './CommentAttachments';
import { useAgents, type Agent } from '../agents/useAgents';
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

function slugifyAgentName(input: string): string {
  const lower = input.toLowerCase();
  let out = '';
  let prevHyphen = false;
  for (const ch of lower) {
    if (/[a-z0-9]/.test(ch)) {
      out += ch;
      prevHyphen = false;
    } else if (!prevHyphen) {
      out += '-';
      prevHyphen = true;
    }
  }
  return out.replace(/^-+|-+$/g, '');
}

function agentMentionKey(agent: Pick<Agent, 'name'>): string {
  return slugifyAgentName(agent.name);
}

interface MentionMatch {
  start: number;
  query: string;
}

function mentionMatchAtCursor(text: string, cursor: number): MentionMatch | null {
  const before = text.slice(0, cursor);
  const atIndex = before.lastIndexOf('@');
  if (atIndex === -1) return null;

  const query = before.slice(atIndex + 1);
  if (/\s/.test(query)) return null;

  return { start: atIndex, query };
}

const MENTION_MODE_TOOLTIP = (
  <>
    <span className="block">
      <strong className="font-medium text-text-primary">Agent:</strong> will do the
      work you ask for.
    </span>
    <span className="block">
      <strong className="font-medium text-text-primary">Chat:</strong> will reply in
      the comment thread.
    </span>
  </>
);

export function TicketCommentsTab({ ticketId }: TicketCommentsTabProps) {
  const { data: comments, isLoading, isError } = useComments(ticketId);
  const { data: agents } = useAgents();
  const toast = useToast();
  const agentNamesById = useMemo(
    () => new Map((agents ?? []).map((agent) => [agent.id, agent.name])),
    [agents],
  );
  const agentKeyOptions = useMemo(
    () =>
      (agents ?? [])
        .filter((agent) => agent.enabled)
        .map((agent) => ({
          key: agentMentionKey(agent),
          name: agent.name,
        }))
        .sort((a, b) => a.name.localeCompare(b.name)),
    [agents],
  );
  const createComment = useCreateComment(ticketId);
  const uploadAttachment = useUploadAttachment();
  const fileInputRef = useRef<HTMLInputElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const [body, setBody] = useState('');
  const [mentionMode, setMentionMode] = useState<MentionMode>('agent');
  const [mentionMatch, setMentionMatch] = useState<MentionMatch | null>(null);
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

  function syncMentionMatch(value: string, cursor: number) {
    setMentionMatch(mentionMatchAtCursor(value, cursor));
  }

  function handleBodyChange(e: React.ChangeEvent<HTMLTextAreaElement>) {
    const value = e.target.value;
    setBody(value);
    syncMentionMatch(value, e.target.selectionStart);
  }

  function handleBodySelect(e: React.SyntheticEvent<HTMLTextAreaElement>) {
    syncMentionMatch(e.currentTarget.value, e.currentTarget.selectionStart);
  }

  function insertMention(key: string) {
    if (!mentionMatch) return;

    const cursor = textareaRef.current?.selectionStart ?? body.length;
    const before = body.slice(0, mentionMatch.start);
    const after = body.slice(cursor);
    const next = `${before}@${key} ${after}`;
    const nextCursor = before.length + key.length + 2;

    setBody(next);
    setMentionMatch(null);

    requestAnimationFrame(() => {
      const textarea = textareaRef.current;
      if (!textarea) return;
      textarea.focus();
      textarea.setSelectionRange(nextCursor, nextCursor);
    });
  }

  const filteredMentionKeys = useMemo(() => {
    if (!mentionMatch) return [];
    const query = mentionMatch.query.toLowerCase();
    return agentKeyOptions.filter(
      (option) =>
        option.key.toLowerCase().includes(query) ||
        option.name.toLowerCase().includes(query),
    );
  }, [agentKeyOptions, mentionMatch]);

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

      const result = await createComment.mutateAsync({
        body: trimmed,
        attachmentIds: attachmentIds.length > 0 ? attachmentIds : undefined,
        mentionMode,
      });

      if (result.startedRuns?.length) {
        for (const run of result.startedRuns) {
          toast.success(`Started run for ${run.agentKey}`);
        }
      }

      setBody('');
      setMentionMatch(null);
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
      <form
        onSubmit={(e) => void handleSubmit(e)}
        className="shrink-0 space-y-3 border-b border-border pb-4"
      >
        {error && (
          <p className="rounded-md border border-danger-muted bg-danger-muted/40 px-3 py-2 font-body text-sm text-danger">
            {error}
          </p>
        )}

        <div className="space-y-2">
          <div className="group relative w-fit">
            <Select
              value={mentionMode}
              onValueChange={(value) => setMentionMode(value as MentionMode)}
            >
              <SelectTrigger
                aria-label="Mention mode"
                aria-describedby="mention-mode-tooltip"
                className="h-8 w-[6.75rem] shrink-0"
              >
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="agent">Agent</SelectItem>
                <SelectItem value="chat">Chat</SelectItem>
              </SelectContent>
            </Select>
            <span
              id="mention-mode-tooltip"
              role="tooltip"
              className="pointer-events-none absolute left-0 top-full z-20 mt-1.5 hidden w-max max-w-xs space-y-1 rounded-md border border-border bg-surface-raised px-2.5 py-1.5 font-body text-xs leading-snug text-text-secondary shadow-sm group-hover:block group-focus-within:block"
            >
              {MENTION_MODE_TOOLTIP}
            </span>
          </div>

          <div className="relative">
            <textarea
              ref={textareaRef}
              value={body}
              onChange={handleBodyChange}
              onSelect={handleBodySelect}
              onClick={handleBodySelect}
              rows={4}
              placeholder="Write a comment in markdown…"
              className="field-control w-full px-3 py-2 font-body text-sm"
            />

            {mentionMatch && filteredMentionKeys.length > 0 && (
              <ul
                role="listbox"
                aria-label="Agent mentions"
                className="absolute bottom-full left-0 z-10 mb-1 max-h-40 w-full overflow-y-auto rounded-md border border-border bg-surface-raised shadow-md"
              >
                {filteredMentionKeys.map((option) => (
                  <li key={option.key}>
                    <button
                      type="button"
                      role="option"
                      onMouseDown={(e) => e.preventDefault()}
                      onClick={() => insertMention(option.key)}
                      className="flex w-full items-center justify-between gap-2 px-3 py-2 text-left font-body text-sm hover:bg-surface"
                    >
                      <span className="truncate font-medium text-text-primary">
                        {option.name}
                      </span>
                      <span className="shrink-0 font-mono text-xs text-text-muted">
                        @{option.key}
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>

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

      <div className="min-h-0 flex-1 space-y-3 overflow-y-auto">
        {isLoading && (
          <p className="font-body text-sm text-text-muted">Loading comments…</p>
        )}

        {isError && (
          <p className="font-body text-sm text-danger">Unable to load comments.</p>
        )}

        {!isLoading && !isError && (comments?.length ?? 0) === 0 && (
          <p className="font-body text-sm text-text-muted">No comments yet.</p>
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
            <TicketMarkdown>{comment.body}</TicketMarkdown>
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
    </div>
  );
}
