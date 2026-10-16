import { useEffect, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { GitBranch } from 'lucide-react';
import type { TicketParentSummary } from '../board/ticketHierarchy';
import type { Ticket } from '../board/useTickets';
import { MarkdownEditor } from '../../components/MarkdownEditor';
import { TicketMarkdown } from '../../components/TicketMarkdown';
import { useToast } from '../../components/ToastProvider';
import {
  attachmentFailedMessage,
  imageFilesFromClipboard,
  isFileDrag,
  partitionAttachmentFiles,
} from '../../lib/attachmentFiles';
import type { AttachmentMeta } from '../../lib/attachments';
import { Button } from '../../components/ui/button';
import { Input } from '../../components/ui/input';
import { PlanChecklist } from '../plan/PlanChecklist';
import { PlanReviewActions } from '../plan/PlanReviewActions';
import { AttachFilesButton } from './AttachFilesButton';
import { CommentAttachments } from './CommentAttachments';
import { TicketStatusBadge } from './TicketStatusBadge';
import { RestartChoice } from './RestartChoice';
import { TicketCommentsTab } from './TicketCommentsTab';
import { useTicketChildren, useUpdateTicket, useUploadAttachment } from './useTicket';

interface TicketDetailPanelProps {
  ticket: Ticket;
  parentTicket?: TicketParentSummary | null;
}

export function TicketDetailPanel({
  ticket,
  parentTicket = null,
}: TicketDetailPanelProps) {
  const toast = useToast();
  const [, setSearchParams] = useSearchParams();
  const updateTicket = useUpdateTicket(ticket.id);
  const uploadAttachment = useUploadAttachment();
  const { data: children } = useTicketChildren(ticket.id);
  const [editing, setEditing] = useState(false);
  const [dropActive, setDropActive] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [title, setTitle] = useState(ticket.title);
  const [description, setDescription] = useState(ticket.description);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setTitle(ticket.title);
    setDescription(ticket.description);
  }, [ticket.title, ticket.description]);

  async function handleSave() {
    setError(null);
    setIsSaving(true);
    try {
      await updateTicket.mutateAsync({
        title: title.trim(),
        description,
      });
      setEditing(false);
      toast.success('Ticket saved');
    } catch {
      setError('Unable to save changes.');
      toast.error('Unable to save ticket');
    } finally {
      setIsSaving(false);
    }
  }

  const saving = isSaving || updateTicket.isPending;
  const isArchived = Boolean(ticket.archivedAt);
  const descriptionFiles = descriptionAttachments(ticket);

  async function attachToTicket(incoming: File[]) {
    if (isArchived) return;
    const { accepted, errors } = partitionAttachmentFiles(incoming);
    for (const message of errors) toast.error(message);
    const uploaded: string[] = [];
    const linked: File[] = [];
    for (const file of accepted) {
      try {
        const result = await uploadAttachment.mutateAsync(file);
        uploaded.push(result.id);
        linked.push(file);
      } catch {
        toast.error(attachmentFailedMessage(file));
      }
    }
    if (uploaded.length === 0) return;
    try {
      await updateTicket.mutateAsync({
        attachmentIds: [...(ticket.attachmentIds ?? []), ...uploaded],
      });
    } catch {
      for (const file of linked) toast.error(attachmentFailedMessage(file));
    }
  }

  function handleCancel() {
    setTitle(ticket.title);
    setDescription(ticket.description);
    setEditing(false);
    setError(null);
  }

  return (
    <div
      className="relative flex min-h-0 flex-col gap-8"
      onDragEnter={(event) => {
        if (!isFileDrag(event.dataTransfer)) return;
        event.preventDefault();
      }}
      onDragOver={(event) => {
        if (!isFileDrag(event.dataTransfer)) return;
        event.preventDefault();
        const overComment = (event.target as HTMLElement).closest?.(
          '[data-attachment-zone="comment"]',
        );
        setDropActive(!overComment);
      }}
      onDragLeave={(event) => {
        if (event.currentTarget.contains(event.relatedTarget as Node)) return;
        setDropActive(false);
      }}
      onDrop={(event) => {
        if (!isFileDrag(event.dataTransfer)) return;
        const overComment = (event.target as HTMLElement).closest?.(
          '[data-attachment-zone="comment"]',
        );
        setDropActive(false);
        if (overComment) return;
        event.preventDefault();
        void attachToTicket(Array.from(event.dataTransfer.files));
      }}
    >
      {dropActive && (
        <div className="pointer-events-none absolute inset-0 z-20 flex items-center justify-center rounded-md border-2 border-dashed border-accent bg-paper-50/90">
          <p className="font-body text-sm font-medium text-text-primary">
            Drop to attach
          </p>
        </div>
      )}
      <div className="flex flex-col gap-4">
        <div className="flex items-start justify-between gap-4">
          {editing ? (
            <Input
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              className="font-display text-lg font-semibold"
            />
          ) : (
            <h3 className="font-display text-lg font-semibold text-text-primary">
              {ticket.title}
            </h3>
          )}

          <div className="flex shrink-0 gap-2">
            {editing ? (
              <>
                <Button
                  type="button"
                  variant="secondary"
                  onClick={handleCancel}
                  disabled={saving}
                >
                  Cancel
                </Button>
                <Button
                  type="button"
                  onClick={() => void handleSave()}
                  loading={saving}
                  disabled={saving || title.trim().length === 0}
                >
                  {saving ? 'Saving…' : 'Save'}
                </Button>
              </>
            ) : (
              <Button
                type="button"
                variant="secondary"
                onClick={() => setEditing(true)}
                disabled={isArchived}
                title={
                  isArchived
                    ? 'Unarchive this ticket before editing'
                    : undefined
                }
              >
                Edit
              </Button>
            )}
          </div>
        </div>

        {error && (
          <p className="rounded-md border border-danger-muted bg-danger-muted/40 px-3 py-2 font-body text-sm text-danger">
            {error}
          </p>
        )}

        <div
          onPaste={(event) => {
            const images = imageFilesFromClipboard(event.clipboardData);
            if (images.length === 0) return;
            event.preventDefault();
            void attachToTicket(images);
          }}
        >
          {editing ? (
            <MarkdownEditor
              value={description}
              onChange={setDescription}
              placeholder="Describe the ticket in markdown…"
              aria-label="Ticket description"
              minHeight={200}
              disabled={saving}
              onSubmit={() => {
                if (!saving && title.trim().length > 0) void handleSave();
              }}
            />
          ) : (
            <div className="min-h-[200px] overflow-y-auto rounded-md border border-border bg-surface px-4 py-3">
              {description.trim() ? (
                <TicketMarkdown>{description}</TicketMarkdown>
              ) : (
                <p className="font-body text-sm italic text-text-muted">
                  No description yet.
                </p>
              )}
            </div>
          )}
          <div className="mt-2">
            <AttachFilesButton
              disabled={isArchived || saving || uploadAttachment.isPending}
              onFiles={(files) => void attachToTicket(files)}
            />
          </div>
          <CommentAttachments attachments={descriptionFiles} />
        </div>
      </div>

      {ticket.status === 'blocked' && !isArchived && (
        <RestartChoice ticketId={ticket.id} boardId={ticket.boardId} />
      )}

      {ticket.status === 'plan_review' && !isArchived && (
        <PlanReviewActions ticketId={ticket.id} boardId={ticket.boardId} />
      )}

      {ticket.status === 'wait_for_final_review' && ticket.approvedPlan && (
        <PlanChecklist plan={ticket.approvedPlan} />
      )}

      {parentTicket && ticket.parentTicketId === parentTicket.id && (
        <section className="border-t border-border pt-6">
          <h3 className="mb-4 font-display text-base font-semibold tracking-tight text-text-primary">
            Parent ticket
          </h3>
          <button
            type="button"
            aria-label={`Open parent ticket: ${parentTicket.title}`}
            onClick={() => setSearchParams({ ticket: parentTicket.id })}
            className="flex min-h-11 w-full min-w-0 items-center gap-2 rounded-md border border-border bg-surface px-3 py-2 text-left transition-colors duration-fast hover:bg-paper-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2"
          >
            <GitBranch
              className="h-4 w-4 shrink-0 text-text-secondary"
              aria-hidden="true"
            />
            <span
              className="min-w-0 truncate font-body text-sm font-medium text-text-primary"
              title={parentTicket.title}
            >
              {parentTicket.title}
            </span>
          </button>
        </section>
      )}

      {children && children.length > 0 && (
        <section className="border-t border-border pt-6">
          <h3 className="mb-4 font-display text-base font-semibold tracking-tight text-text-primary">
            Child tickets
          </h3>
          <ul className="space-y-2">
            {children.map((child) => (
              <li key={child.id}>
                <button
                  type="button"
                  onClick={() => setSearchParams({ ticket: child.id })}
                  className="flex min-h-11 w-full items-center justify-between gap-3 rounded-md border border-border bg-surface px-3 py-2 text-left transition-colors duration-fast hover:bg-paper-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2"
                >
                  <span className="truncate font-body text-sm font-medium text-text-primary">
                    {child.title}
                  </span>
                  <TicketStatusBadge status={child.status} />
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}

      <section className="border-t border-border pt-6">
        <h3 className="mb-4 font-display text-base font-semibold tracking-tight text-text-primary">
          Comments
        </h3>
        <TicketCommentsTab ticketId={ticket.id} />
      </section>
    </div>
  );
}

function descriptionAttachments(ticket: Ticket): AttachmentMeta[] {
  if (ticket.attachments && ticket.attachments.length > 0) {
    return ticket.attachments;
  }
  return (ticket.attachmentIds ?? []).map((id) => ({
    id,
    filename: 'Attachment',
    contentType: 'application/octet-stream',
    sizeBytes: 0,
  }));
}
