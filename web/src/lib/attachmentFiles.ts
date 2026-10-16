import { formatFileSize } from './attachments';

/** Matches the server default `storage.max_upload_bytes`. */
export const MAX_ATTACHMENT_BYTES = 10 * 1024 * 1024;

const ALLOWED_TYPES = new Set([
  'image/png',
  'image/jpeg',
  'image/gif',
  'image/webp',
  'text/plain',
  'text/markdown',
  'text/csv',
  'application/json',
  'application/pdf',
]);

const ALLOWED_EXTENSIONS = new Set([
  'png',
  'jpg',
  'jpeg',
  'gif',
  'webp',
  'txt',
  'md',
  'markdown',
  'csv',
  'json',
  'pdf',
]);

function extensionOf(name: string): string | null {
  const dot = name.lastIndexOf('.');
  if (dot <= 0 || dot === name.length - 1) return null;
  return name.slice(dot + 1).toLowerCase();
}

function fileTypeLabel(file: File): string {
  const ext = extensionOf(file.name);
  if (ext) return ext;
  const subtype = file.type.split('/')[1]?.split('+')[0]?.trim().toLowerCase();
  if (subtype) return subtype;
  return 'that';
}

export function isAllowedAttachment(file: File): boolean {
  const type = file.type.split(';')[0]?.trim().toLowerCase() ?? '';
  if (type && ALLOWED_TYPES.has(type)) return true;
  const ext = extensionOf(file.name);
  return ext !== null && ALLOWED_EXTENSIONS.has(ext);
}

export function attachmentTooLargeMessage(file: File): string {
  return `${file.name} is too large. The limit is ${formatFileSize(MAX_ATTACHMENT_BYTES)}.`;
}

export function attachmentTypeMessage(file: File): string {
  return `Coppice can't attach ${fileTypeLabel(file)} files.`;
}

export function attachmentFailedMessage(file: File): string {
  return `Couldn't attach ${file.name}. Try again.`;
}

/** A user-facing rejection, or null when the file can be uploaded. */
export function attachmentRejection(file: File): string | null {
  if (file.size > MAX_ATTACHMENT_BYTES) return attachmentTooLargeMessage(file);
  if (!isAllowedAttachment(file)) return attachmentTypeMessage(file);
  return null;
}

export function partitionAttachmentFiles(files: readonly File[]): {
  accepted: File[];
  errors: string[];
} {
  const accepted: File[] = [];
  const errors: string[] = [];
  for (const file of files) {
    const error = attachmentRejection(file);
    if (error) errors.push(error);
    else accepted.push(file);
  }
  return { accepted, errors };
}

export function isFileDrag(data: DataTransfer | null): boolean {
  if (!data) return false;
  return Array.from(data.types).includes('Files');
}

export function imageFilesFromClipboard(data: DataTransfer | null): File[] {
  if (!data) return [];
  const files: File[] = [];
  for (const item of Array.from(data.items)) {
    if (item.kind === 'file' && item.type.startsWith('image/')) {
      const file = item.getAsFile();
      if (file) files.push(file);
    }
  }
  return files;
}
