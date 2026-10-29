import { createContext, useContext } from 'react';
import type { FileReference } from './fileReference';

export interface FilePreviewContextValue {
  openFile: (ref: FileReference) => void;
}

export const FilePreviewContext = createContext<FilePreviewContextValue | null>(null);

/** Null outside a ticket with a repository: file paths then render as plain text. */
export function useFilePreview(): FilePreviewContextValue | null {
  return useContext(FilePreviewContext);
}
