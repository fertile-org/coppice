import { useMemo, useState, type ReactNode } from 'react';
import type { FileReference } from './fileReference';
import { FilePreviewContext, type FilePreviewContextValue } from './filePreviewContext';
import { FilePreviewModal } from './FilePreviewModal';

/**
 * Makes file paths in ticket markdown clickable. Pass `ticketId` only when the
 * ticket has a repository; files load from its worktree, else the repo checkout.
 */
export function FilePreviewProvider({
  ticketId,
  children,
}: {
  ticketId: string | null;
  children: ReactNode;
}) {
  const [file, setFile] = useState<FileReference | null>(null);
  const [open, setOpen] = useState(false);
  const value = useMemo<FilePreviewContextValue>(
    () => ({
      openFile: (ref) => {
        setFile(ref);
        setOpen(true);
      },
    }),
    [],
  );

  if (!ticketId) return <>{children}</>;
  return (
    <FilePreviewContext.Provider value={value}>
      {children}
      {file && (
        <FilePreviewModal
          open={open}
          ticketId={ticketId}
          file={file}
          onClose={() => setOpen(false)}
        />
      )}
    </FilePreviewContext.Provider>
  );
}
