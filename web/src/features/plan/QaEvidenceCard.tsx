import { useMemo, useState } from 'react';
import type { ApprovedPlan, PlanCriterion } from '../board/useTickets';
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '../../components/ui/dialog';
import { PLAN_COPY } from './copy';

const THUMB_CAP = 3;

const STATUS_ICON: Record<PlanCriterion['status'], string> = {
  met: '✓',
  not_met: '✕',
  not_checked: '–',
};

const STATUS_CLASS: Record<PlanCriterion['status'], string> = {
  met: 'text-success',
  not_met: 'text-danger',
  not_checked: 'text-warning',
};

export function qaEvidenceUrl(ticketId: string, file: string): string {
  return `/api/tickets/${ticketId}/qa-evidence/${encodeURIComponent(file)}`;
}

function shortSha(sha: string): string {
  return sha.length > 7 ? sha.slice(0, 7) : sha;
}

function pairedCaption(criterion: PlanCriterion, index: number): string | undefined {
  const shots = criterion.screenshots ?? [];
  const steps = criterion.steps ?? [];
  if (shots.length === steps.length) return steps[index];
  return undefined;
}

interface LightboxState {
  file: string;
  criterion: PlanCriterion;
  index: number;
}

export function QaEvidenceCard({
  ticketId,
  plan,
}: {
  ticketId: string;
  plan: ApprovedPlan;
}) {
  const criteria = plan.criteria ?? [];
  const [lightbox, setLightbox] = useState<LightboxState | null>(null);

  const met = criteria.filter((c) => c.status === 'met').length;
  const screenshotCount = useMemo(
    () => criteria.reduce((n, c) => n + (c.screenshots?.length ?? 0), 0),
    [criteria],
  );

  if (!plan.criteriaCommitSha) return null;

  const firstShot = criteria.flatMap((c) => c.screenshots ?? [])[0];

  return (
    <section
      aria-label={PLAN_COPY.qaEvidence}
      className="border-t border-border pt-6"
      data-testid="qa-evidence-card"
    >
      <h3 className="mb-3 flex flex-wrap items-center gap-2 font-display text-base font-semibold tracking-tight text-text-primary">
        <span>{PLAN_COPY.qaEvidence}</span>
        <span className="rounded-full bg-success-muted px-2 py-0.5 font-body text-xs font-medium text-success">
          {PLAN_COPY.qaEvidenceMetBadge(met, criteria.length)}
        </span>
        <span className="font-body text-xs font-normal text-text-muted">
          {PLAN_COPY.qaEvidenceCheckedCommit(shortSha(plan.criteriaCommitSha))}
        </span>
        {screenshotCount > 0 && firstShot ? (
          <button
            type="button"
            className="ml-auto font-body text-sm font-normal text-accent hover:underline"
            onClick={() => {
              const host = criteria.find((c) => (c.screenshots?.length ?? 0) > 0);
              if (!host?.screenshots?.[0]) return;
              setLightbox({ file: host.screenshots[0], criterion: host, index: 0 });
            }}
          >
            {PLAN_COPY.qaEvidenceViewScreenshots(screenshotCount)}
          </button>
        ) : null}
      </h3>

      <ul className="rounded-lg border border-border bg-paper-50 px-3.5 py-1">
        {criteria.map((criterion, index) => (
          <CriterionEvidenceRow
            key={`${index}-${criterion.text}`}
            ticketId={ticketId}
            criterion={criterion}
            onOpen={(file, shotIndex) =>
              setLightbox({ file, criterion, index: shotIndex })
            }
          />
        ))}
      </ul>

      <ScreenshotLightbox
        ticketId={ticketId}
        state={lightbox}
        onClose={() => setLightbox(null)}
      />
    </section>
  );
}

function CriterionEvidenceRow({
  ticketId,
  criterion,
  onOpen,
}: {
  ticketId: string;
  criterion: PlanCriterion;
  onOpen: (file: string, index: number) => void;
}) {
  const shots = criterion.screenshots ?? [];
  const shown = shots.slice(0, THUMB_CAP);
  const extra = shots.length - shown.length;

  return (
    <li className="flex gap-2.5 border-t border-border/60 py-2.5 first:border-t-0">
      <span
        className={`w-4 shrink-0 text-center font-body text-sm ${STATUS_CLASS[criterion.status]}`}
        aria-hidden
      >
        {STATUS_ICON[criterion.status]}
      </span>
      <div className="min-w-0 flex-1">
        <p className="font-body text-sm text-text-primary">{criterion.text}</p>
        {criterion.evidence ? (
          <p className="mt-0.5 font-body text-xs text-text-muted">— {criterion.evidence}</p>
        ) : null}
        {shown.length > 0 ? (
          <div className="mt-2 flex flex-wrap items-center gap-2">
            {shown.map((file, index) => (
              <button
                key={file}
                type="button"
                className="overflow-hidden rounded border border-border bg-paper-100"
                onClick={() => onOpen(file, index)}
                aria-label={`${PLAN_COPY.qaEvidenceLightbox}: ${file}`}
              >
                <img
                  src={qaEvidenceUrl(ticketId, file)}
                  alt=""
                  className="h-[72px] w-[120px] object-cover"
                />
              </button>
            ))}
            {extra > 0 ? (
              <span className="font-body text-xs text-text-muted">
                {PLAN_COPY.qaEvidenceMore(extra)}
              </span>
            ) : null}
          </div>
        ) : null}
      </div>
    </li>
  );
}

function ScreenshotLightbox({
  ticketId,
  state,
  onClose,
}: {
  ticketId: string;
  state: LightboxState | null;
  onClose: () => void;
}) {
  const open = state != null;
  const caption = state ? pairedCaption(state.criterion, state.index) : undefined;
  const unpairedSteps =
    state &&
    (state.criterion.screenshots?.length ?? 0) !== (state.criterion.steps?.length ?? 0)
      ? (state.criterion.steps ?? [])
      : [];

  return (
    <Dialog open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogContent className="flex max-h-[90vh] max-w-3xl flex-col overflow-hidden bg-surface-raised p-0">
        <header className="flex shrink-0 items-center justify-between gap-3 border-b border-border px-4 py-3">
          <div className="min-w-0">
            <DialogTitle className="truncate font-body text-sm font-medium text-text-primary">
              {state?.criterion.text ?? PLAN_COPY.qaEvidenceLightbox}
            </DialogTitle>
            <DialogDescription className="text-xs text-text-muted">
              {caption ?? state?.file ?? ''}
            </DialogDescription>
          </div>
          <DialogClose className="rounded-md border border-border px-2 py-1 font-body text-sm text-text-secondary hover:text-text-primary">
            Close
          </DialogClose>
        </header>
        <div className="flex min-h-0 flex-1 flex-col items-center overflow-auto bg-paper-100 p-4">
          {state ? (
            <img
              src={qaEvidenceUrl(ticketId, state.file)}
              alt={caption ?? state.file}
              className="max-h-[60vh] max-w-full rounded-md object-contain"
            />
          ) : null}
          {unpairedSteps.length > 0 ? (
            <ol className="mt-4 w-full list-decimal space-y-1 pl-5 font-body text-sm text-text-secondary">
              {unpairedSteps.map((step, i) => (
                <li key={`${i}-${step}`}>{step}</li>
              ))}
            </ol>
          ) : null}
        </div>
      </DialogContent>
    </Dialog>
  );
}
