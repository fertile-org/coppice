import { cn } from '../../lib/utils';
import {
  TEXT_SIZE_LABELS,
  TEXT_SIZES,
  type TextSize,
} from '../theme/textSize';
import { useTextSize } from '../theme/TextSizeProvider';

export function AppearanceSection() {
  const { size, setSize } = useTextSize();

  return (
    <section aria-label="Appearance" className="space-y-3">
      <h2 className="border-b border-border pb-2 font-display text-lg font-semibold text-text-primary">
        Appearance
      </h2>
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <div className="font-body text-sm font-medium text-text-primary">Text size</div>
          <p className="mt-0.5 font-body text-sm text-text-muted">
            Applies on this device only. Takes effect right away.
          </p>
        </div>
        <div
          className="inline-flex overflow-hidden rounded-md border border-border"
          role="group"
          aria-label="Text size"
          data-testid="text-size-control"
        >
          {TEXT_SIZES.map((value, index) => (
            <button
              key={value}
              type="button"
              aria-pressed={size === value}
              onClick={() => setSize(value)}
              className={cn(
                'px-3.5 py-1.5 font-body text-sm transition-colors duration-fast',
                index > 0 && 'border-l border-border',
                size === value
                  ? 'bg-moss-600 font-semibold text-paper-50'
                  : 'text-text-primary hover:bg-paper-200',
              )}
            >
              {TEXT_SIZE_LABELS[value]}
            </button>
          ))}
        </div>
      </div>
      <TextSizePreview size={size} />
    </section>
  );
}

function TextSizePreview({ size }: { size: TextSize }) {
  return (
    <div
      data-testid="text-size-preview"
      className="rounded-md border border-border bg-surface-raised px-4 py-3"
      style={{ fontSize: `${size}px` }}
    >
      <h3 className="font-display text-[1.15em] font-semibold text-text-primary">
        Approved plan
      </h3>
      <p className="mt-2 font-body leading-relaxed text-text-primary">
        The QC agent attaches screenshots to Human Review only when it can find a browser. Add
        two or three sentences in the two places a reader already looks, and nothing else.
      </p>
      <p className="mt-2 font-body text-[0.9em] text-text-muted">
        Getting started says browser screenshots are optional.
      </p>
    </div>
  );
}
