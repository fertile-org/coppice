import type { ReviewCapacity } from './useReviewCapacity';

export function ReviewWaitingChip({
  saved,
}: {
  saved: ReviewCapacity | undefined;
}) {
  if (!saved?.waitingOnReview) return null;
  return (
    <p className="mt-3">
      <span
        data-testid="review-waiting-chip"
        className="inline-flex rounded-full border border-border bg-paper-200 px-2.5 py-0.5 font-body text-xs text-text-secondary"
      >
        {saved.waitingOnReview}
      </span>
    </p>
  );
}
