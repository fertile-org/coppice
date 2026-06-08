export interface AttachmentMeta {
  id: string;
  filename: string;
  contentType: string;
  sizeBytes: number;
}

export function attachmentUrl(id: string): string {
  return `/api/attachments/${id}`;
}

export function isImageContentType(contentType: string): boolean {
  return contentType.startsWith('image/');
}

export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
