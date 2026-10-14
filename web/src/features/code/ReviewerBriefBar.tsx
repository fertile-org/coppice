import { REVIEWER_BRIEF_COPY } from './reviewerBriefCopy';

export interface ReviewerBrief {
  howItWorks?: string | null;
  changesFromPlan?: string | null;
  lookHereFirst?: string | null;
}

function shown(value: string | null | undefined, empty: string): string {
  const text = value?.trim() ?? '';
  return text.length > 0 ? text : empty;
}

export function ReviewerBriefBar({ brief }: { brief: ReviewerBrief }) {
  return (
    <section
      aria-label={REVIEWER_BRIEF_COPY.section}
      data-testid="reviewer-brief"
      className="shrink-0 border-b border-border px-4 py-3"
    >
      <dl className="space-y-1">
        <BriefRow
          label={REVIEWER_BRIEF_COPY.howItWorks}
          value={shown(brief.howItWorks, '')}
        />
        <BriefRow
          label={REVIEWER_BRIEF_COPY.changesFromPlan}
          value={shown(brief.changesFromPlan, REVIEWER_BRIEF_COPY.none)}
        />
        <BriefRow
          label={REVIEWER_BRIEF_COPY.lookHereFirst}
          value={shown(brief.lookHereFirst, '')}
        />
      </dl>
    </section>
  );
}

function BriefRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex min-w-0 gap-3">
      <dt className="shrink-0 font-body text-xs text-text-muted">{label}</dt>
      <dd className="line-clamp-2 min-w-0 font-body text-sm text-text-primary">
        {value}
      </dd>
    </div>
  );
}
