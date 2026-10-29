import type { ReactNode } from 'react';
import { formatFileReference, type FileReference } from './fileReference';
import { useFilePreview } from './filePreviewContext';

/** A mentioned repo file. Without a preview provider it renders its children unchanged. */
export function FileReferenceLink({
  file,
  children,
}: {
  file: FileReference;
  children: ReactNode;
}) {
  const preview = useFilePreview();
  if (!preview) return <>{children}</>;
  return (
    <button
      type="button"
      onClick={() => preview.openFile(file)}
      title={`Preview ${formatFileReference(file)}`}
      className="inline cursor-pointer rounded text-left text-accent underline decoration-accent/40 decoration-dotted underline-offset-2 transition-colors duration-fast hover:decoration-solid focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/50"
    >
      {children}
    </button>
  );
}
