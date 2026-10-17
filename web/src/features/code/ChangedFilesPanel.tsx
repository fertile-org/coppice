import { useVirtualizer } from '@tanstack/react-virtual';
import { useEffect, useRef, type ReactNode } from 'react';
import { Badge } from '../../components/ui/badge';
import type { DiffFileSummary } from '../../lib/schemas/codeReview';
import { cn } from '../../lib/utils';
import { REVIEWER_BRIEF_COPY } from './reviewerBriefCopy';

function statusLabel(status: string): string {
  switch (status.toLowerCase()) {
    case 'added':
      return 'A';
    case 'deleted':
      return 'D';
    case 'renamed':
      return 'R';
    case 'copied':
      return 'C';
    default:
      return 'M';
  }
}

function statusClass(status: string): string {
  switch (status.toLowerCase()) {
    case 'added':
      return 'border-success-muted bg-success-muted text-success';
    case 'deleted':
      return 'border-danger-muted bg-danger-muted text-danger';
    case 'renamed':
    case 'copied':
      return 'border-accent-muted bg-accent-muted text-accent';
    default:
      return 'border-border bg-surface text-text-secondary';
  }
}

interface ChangedFilesPanelProps {
  files: DiffFileSummary[];
  selectedPath: string | undefined;
  onSelect: (path: string) => void;
  isLoading?: boolean;
}

/** File lists longer than this mount only the rows in view. */
export const VIRTUALIZE_FILES_AFTER = 200;
const FILE_ROW_HEIGHT = 52;

function FileList({
  files,
  selectedPath,
  renderFile,
}: {
  files: DiffFileSummary[];
  selectedPath: string | null | undefined;
  renderFile: (file: DiffFileSummary) => ReactNode;
}) {
  if (files.length <= VIRTUALIZE_FILES_AFTER) {
    return (
      <ul className="min-h-0 flex-1 overflow-y-auto py-1">
        {files.map((file) => (
          <li key={file.path}>{renderFile(file)}</li>
        ))}
      </ul>
    );
  }
  return <VirtualFileList files={files} selectedPath={selectedPath} renderFile={renderFile} />;
}

function VirtualFileList({
  files,
  selectedPath,
  renderFile,
}: {
  files: DiffFileSummary[];
  selectedPath: string | null | undefined;
  renderFile: (file: DiffFileSummary) => ReactNode;
}) {
  const parentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: files.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => FILE_ROW_HEIGHT,
    overscan: 10,
    initialRect: { width: 280, height: 800 },
    getItemKey: (index) => files[index]?.path ?? index,
  });
  const selectedIndex = selectedPath ? files.findIndex((file) => file.path === selectedPath) : -1;
  useEffect(() => {
    if (selectedIndex >= 0) virtualizer.scrollToIndex(selectedIndex, { align: 'auto' });
  }, [selectedIndex, virtualizer]);
  return (
    <div ref={parentRef} data-virtual-files="" className="min-h-0 flex-1 overflow-y-auto py-1">
      <ul className="relative w-full" style={{ height: virtualizer.getTotalSize() }}>
        {virtualizer.getVirtualItems().map((item) => {
          const file = files[item.index];
          if (!file) return null;
          return (
            <li
              key={item.key}
              className="absolute left-0 top-0 w-full"
              style={{ height: item.size, transform: `translateY(${item.start}px)` }}
            >
              {renderFile(file)}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

export function ChangedFilesPanel({
  files,
  selectedPath,
  onSelect,
  isLoading,
}: ChangedFilesPanelProps) {
  if (isLoading) {
    return (
      <div className="flex h-full flex-col border-r border-border bg-surface-raised">
        <div className="border-b border-border px-4 py-3">
          <h2 className="font-body text-sm font-medium text-text-primary">
            Changed files
          </h2>
        </div>
        <p className="px-4 py-6 font-body text-sm text-text-secondary">
          Loading…
        </p>
      </div>
    );
  }

  if (files.length === 0) {
    return (
      <div className="flex h-full flex-col border-r border-border bg-surface-raised">
        <div className="border-b border-border px-4 py-3">
          <h2 className="font-body text-sm font-medium text-text-primary">
            Changed files
          </h2>
        </div>
        <p className="px-4 py-6 font-body text-sm text-text-secondary">
          No changes between base and HEAD.
        </p>
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col border-r border-border bg-surface-raised">
      <div className="shrink-0 border-b border-border px-4 py-3">
        <h2 className="font-body text-sm font-medium text-text-primary">
          Changed files
        </h2>
        <p className="mt-0.5 font-body text-xs text-text-muted">
          {files.length} file{files.length === 1 ? '' : 's'}
        </p>
      </div>
      <FileList
        files={files}
        selectedPath={selectedPath}
        renderFile={(file) => {
          const selected = file.path === selectedPath;
          return (
            <button
              type="button"
              onClick={() => onSelect(file.path)}
              className={cn(
                'flex w-full items-start gap-2 px-3 py-2 text-left transition-colors duration-fast',
                selected
                  ? 'bg-accent-muted/50 text-text-primary'
                  : 'text-text-secondary hover:bg-paper-100 hover:text-text-primary',
              )}
            >
              <Badge
                variant="outline"
                className={cn('mt-0.5 shrink-0 font-mono', statusClass(file.status))}
              >
                {statusLabel(file.status)}
              </Badge>
              <span className="min-w-0 flex-1">
                <span className="flex min-w-0 items-center gap-1">
                  <span className="truncate font-mono text-xs">{file.path}</span>
                  {file.notInPlan && (
                    <span className="shrink-0 rounded border border-border px-1 py-px font-body text-[10px] leading-4 text-text-muted">
                      {REVIEWER_BRIEF_COPY.notInPlan}
                    </span>
                  )}
                </span>
                <span className="mt-0.5 block font-body text-xs text-text-muted">
                  <span className="text-success">+{file.additions}</span>
                  {' · '}
                  <span className="text-danger">−{file.deletions}</span>
                </span>
              </span>
            </button>
          );
        }}
      />
    </div>
  );
}
