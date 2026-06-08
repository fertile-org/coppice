import { useState } from 'react';
import { FileText } from 'lucide-react';
import {
  attachmentUrl,
  formatFileSize,
  isImageContentType,
  type AttachmentMeta,
} from '../../lib/attachments';
import { AttachmentPreviewModal } from './AttachmentPreviewModal';

interface CommentAttachmentsProps {
  attachments: AttachmentMeta[];
}

export function CommentAttachments({ attachments }: CommentAttachmentsProps) {
  const [preview, setPreview] = useState<AttachmentMeta | null>(null);

  if (attachments.length === 0) return null;

  return (
    <>
      <ul className="mt-3 flex flex-wrap gap-2">
        {attachments.map((attachment) => {
          const isImage = isImageContentType(attachment.contentType);
          const url = attachmentUrl(attachment.id);

          return (
            <li key={attachment.id}>
              <button
                type="button"
                onClick={() => setPreview(attachment)}
                className="group block overflow-hidden rounded-md border border-border bg-surface transition-colors duration-fast hover:border-accent"
                title={attachment.filename}
              >
                {isImage ? (
                  <img
                    src={url}
                    alt={attachment.filename}
                    className="size-20 object-cover"
                    loading="lazy"
                  />
                ) : (
                  <div className="flex size-20 flex-col items-center justify-center gap-1 px-2 text-center">
                    <FileText
                      className="size-6 text-text-muted"
                      aria-hidden="true"
                    />
                    <span className="line-clamp-2 font-body text-[10px] leading-tight text-text-secondary">
                      {attachment.filename}
                    </span>
                    <span className="font-body text-[10px] text-text-muted">
                      {formatFileSize(attachment.sizeBytes)}
                    </span>
                  </div>
                )}
              </button>
            </li>
          );
        })}
      </ul>

      {preview && (
        <AttachmentPreviewModal
          attachment={preview}
          onClose={() => setPreview(null)}
        />
      )}
    </>
  );
}
