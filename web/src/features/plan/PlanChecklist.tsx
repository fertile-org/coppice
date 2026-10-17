import type { CriterionStatus, PlanCriterion } from '../board/useTickets';
import { PLAN_COPY } from './copy';

export interface ApprovedPlan {
  commentId: string;
  markdown: string;
  steps: string[];
  criteria?: PlanCriterion[];
}

const STATUS_LABEL: Record<CriterionStatus, string> = {
  met: PLAN_COPY.criterionMet,
  not_met: PLAN_COPY.criterionNotMet,
  not_checked: PLAN_COPY.criterionNotChecked,
};

const STATUS_CLASS: Record<CriterionStatus, string> = {
  met: 'text-text-muted',
  not_met: 'text-danger',
  not_checked: 'text-warning',
};

function CriterionRow({ criterion }: { criterion: PlanCriterion }) {
  const label = STATUS_LABEL[criterion.status];
  return (
    <li title={criterion.evidence ?? undefined}>
      <label className="flex items-start gap-2 font-body text-sm text-text-primary">
        <input
          type="checkbox"
          checked={criterion.status === 'met'}
          disabled
          readOnly
          className="mt-0.5"
        />
        <span className="min-w-0">
          <span>{criterion.text}</span>{' '}
          <span className={`whitespace-nowrap font-mono text-xs ${STATUS_CLASS[criterion.status]}`}>
            {label}
          </span>
          {criterion.evidence && (
            <span className="block truncate text-xs text-text-muted">{criterion.evidence}</span>
          )}
        </span>
      </label>
    </li>
  );
}

export function PlanChecklist({ plan }: { plan: ApprovedPlan }) {
  const criteria = plan.criteria ?? [];
  return (
    <aside
      aria-label={PLAN_COPY.approvedPlan}
      className="min-h-0 overflow-y-auto border-border bg-surface px-4 py-4 md:border-r"
    >
      <h2 className="font-display text-sm font-semibold text-text-primary">
        {PLAN_COPY.approvedPlan}
      </h2>
      {criteria.length > 0 && (
        <section aria-label={PLAN_COPY.criteria} className="mt-3">
          <h3 className="font-body text-xs font-semibold uppercase text-text-muted">
            {PLAN_COPY.criteria}
          </h3>
          <ul className="mt-2 space-y-2">
            {criteria.map((criterion, index) => (
              <CriterionRow key={`${index}-${criterion.text}`} criterion={criterion} />
            ))}
          </ul>
        </section>
      )}
      {criteria.length > 0 && plan.steps.length > 0 && (
        <h3 className="mt-4 font-body text-xs font-semibold uppercase text-text-muted">
          {PLAN_COPY.steps}
        </h3>
      )}
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
