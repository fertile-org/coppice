import { PLAN_COPY } from './copy';

export interface ApprovedPlan {
  commentId: string;
  markdown: string;
  steps: string[];
}

export function PlanChecklist({ plan }: { plan: ApprovedPlan }) {
  return (
    <aside
      aria-label={PLAN_COPY.approvedPlan}
      className="min-h-0 overflow-y-auto border-border bg-surface px-4 py-4 md:border-r"
    >
      <h2 className="font-display text-sm font-semibold text-text-primary">
        {PLAN_COPY.approvedPlan}
      </h2>
      {plan.steps.length === 0 ? (
        <p className="mt-3 font-body text-sm text-text-muted">{PLAN_COPY.noSteps}</p>
      ) : (
        <ul className="mt-3 space-y-2">
          {plan.steps.map((step, index) => (
            <li key={`${index}-${step}`}>
              <label className="flex items-start gap-2 font-body text-sm text-text-primary">
                <input
                  type="checkbox"
                  checked={false}
                  disabled
                  readOnly
                  className="mt-0.5"
                />
                <span>{step}</span>
              </label>
            </li>
          ))}
        </ul>
      )}
    </aside>
  );
}
