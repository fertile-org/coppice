import { X } from 'lucide-react';
import {
  attachmentUrl,
  formatFileSize,
  isImageContentType,
  type AttachmentMeta,
} from '../../lib/attachments';
import { Button } from '../../components/ui/button';
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '../../components/ui/dialog';

interface AttachmentPreviewModalProps {
  /** Keep the modal mounted and toggle this so the exit animation plays. */
  open?: boolean;
  attachment: AttachmentMeta;
  onClose: () => void;
}

export function AttachmentPreviewModal({
  open = true,
  attachment,
  onClose,
}: AttachmentPreviewModalProps) {
  const isImage = isImageContentType(attachment.contentType);
  const url = attachmentUrl(attachment.id);

  return (
    <Dialog open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogContent
        className="z-[110] flex max-h-[90vh] max-w-4xl flex-col overflow-hidden bg-surface-raised p-0 shadow-2xl"
        overlayClassName="z-[110] bg-overlay-strong backdrop-blur-[2px]"
      >
        <header className="flex shrink-0 items-center justify-between gap-3 border-b border-border px-4 py-3">
          <div className="min-w-0">
            <DialogTitle className="truncate font-body text-sm font-medium text-text-primary">
              {attachment.filename}
            </DialogTitle>
            <DialogDescription className="text-xs text-text-muted">
              {formatFileSize(attachment.sizeBytes)}
            </DialogDescription>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Button variant="secondary" size="sm" asChild>
              <a href={url} download={attachment.filename}>
                Download
              </a>
            </Button>
            <DialogClose
              className="rounded-md border border-border p-1.5 text-text-secondary transition-colors duration-fast hover:text-text-primary"
              aria-label="Close preview"
            >
              <X className="size-4" />
            </DialogClose>
          </div>
        </header>

        <div className="flex min-h-0 flex-1 items-center justify-center overflow-auto bg-paper-100 p-4">
          {isImage ? (
            <img
              src={url}
              alt={attachment.filename}
              className="max-h-[70vh] max-w-full rounded-md object-contain shadow-md"
            />
          ) : (
            <div className="flex flex-col items-center gap-3 rounded-lg border border-border bg-surface px-8 py-10 text-center">
              <p className="font-body text-sm text-text-secondary">
                Preview is not available for this file type.
              </p>
              <Button asChild>
                <a href={url} download={attachment.filename}>
                  Download {attachment.filename}
                </a>
              </Button>
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
