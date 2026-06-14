import { Badge } from '../../components/ui/badge';
import type { DiffFileSummary } from '../../lib/schemas/codeReview';
import { cn } from '../../lib/utils';

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
      <ul className="min-h-0 flex-1 overflow-y-auto py-1">
        {files.map((file) => {
          const selected = file.path === selectedPath;
          return (
            <li key={file.path}>
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
                  <span className="block truncate font-mono text-xs">{file.path}</span>
                  <span className="mt-0.5 block font-body text-xs text-text-muted">
                    <span className="text-success">+{file.additions}</span>
                    {' · '}
                    <span className="text-danger">−{file.deletions}</span>
                  </span>
                </span>
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
