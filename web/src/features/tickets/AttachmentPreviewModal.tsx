import { useEffect } from 'react';
import { X } from 'lucide-react';
import {
  attachmentUrl,
  formatFileSize,
  isImageContentType,
  type AttachmentMeta,
} from '../../lib/attachments';
import { Button } from '../../components/ui/button';

interface AttachmentPreviewModalProps {
  attachment: AttachmentMeta;
  onClose: () => void;
}

export function AttachmentPreviewModal({
  attachment,
  onClose,
}: AttachmentPreviewModalProps) {
  const isImage = isImageContentType(attachment.contentType);
  const url = attachmentUrl(attachment.id);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 z-[110] flex items-center justify-center p-4"
      role="presentation"
    >
      <div
        className="absolute inset-0 bg-bark-950/60 backdrop-blur-[2px]"
        onClick={onClose}
        aria-hidden="true"
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-label={attachment.filename}
        className="relative flex max-h-[90vh] w-full max-w-4xl flex-col overflow-hidden rounded-xl border border-border bg-surface-raised shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="flex shrink-0 items-center justify-between gap-3 border-b border-border px-4 py-3">
          <div className="min-w-0">
            <p className="truncate font-body text-sm font-medium text-text-primary">
              {attachment.filename}
            </p>
            <p className="font-body text-xs text-text-muted">
              {formatFileSize(attachment.sizeBytes)}
            </p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Button variant="secondary" size="sm" asChild>
              <a href={url} download={attachment.filename}>
                Download
              </a>
            </Button>
            <button
              type="button"
              onClick={onClose}
              className="rounded-md border border-border p-1.5 text-text-secondary transition-colors duration-fast hover:text-text-primary"
              aria-label="Close preview"
            >
              <X className="size-4" />
            </button>
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
      </div>
    </div>
  );
}
