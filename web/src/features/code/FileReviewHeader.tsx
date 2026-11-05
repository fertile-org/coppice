import { Button } from '../../components/ui/button';
import { splitFilePath } from './reviewNavigation';

export function FileReviewHeader({
  path,
  additions,
  deletions,
  index,
  total,
  viewed,
  onViewedChange,
  onPrevious,
  onNext,
  canPrevious,
  canNext,
}: {
  path: string;
  additions: number;
  deletions: number;
  index: number;
  total: number;
  viewed: boolean;
  onViewedChange: (viewed: boolean) => void;
  onPrevious: () => void;
  onNext: () => void;
  canPrevious: boolean;
  canNext: boolean;
}) {
  const { dir, name } = splitFilePath(path);
  return (
    <div
      data-testid="file-review-header"
      className="sticky top-0 z-10 flex shrink-0 flex-wrap items-center gap-x-3 gap-y-2 border-b border-border bg-surface px-4 py-2"
    >
      <label className="flex items-center gap-2 font-body text-sm text-text-secondary">
        <input
          type="checkbox"
          checked={viewed}
          onChange={(event) => onViewedChange(event.target.checked)}
        />
        Viewed
      </label>
      <p className="flex min-w-0 flex-1 items-baseline font-mono text-sm" title={path}>
        {dir ? (
          <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-left text-text-muted [direction:rtl]">
            <span className="[unicode-bidi:plaintext]">{dir}</span>
          </span>
        ) : null}
        <span className="shrink-0 font-semibold text-text-primary">{name}</span>
      </p>
      <p className="font-mono text-xs">
        <span className="text-success">+{additions}</span>{' '}
        <span className="text-danger">−{deletions}</span>
      </p>
      <div className="ml-auto flex items-center gap-2">
        <span className="font-mono text-xs text-text-muted">
          {index} / {total}
        </span>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          aria-label="Previous file"
          disabled={!canPrevious}
          onClick={onPrevious}
        >
          ‹ Prev <kbd className="font-mono text-[10px] text-text-muted">K</kbd>
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          aria-label="Next file"
          disabled={!canNext}
          onClick={onNext}
        >
          Next <kbd className="font-mono text-[10px] text-text-muted">J</kbd> ›
        </Button>
      </div>
    </div>
  );
}
