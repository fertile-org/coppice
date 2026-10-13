/** Placeholders until Content locks the copy. */

export const MODEL_NOT_REPORTED = 'Model not reported';

export const BILLED_OUTSIDE_PLAN = 'Billed outside your plan';

export const MODELS_NOT_COVERED_HEADING = 'Models not covered by my plan';

/** Placeholder. `{connector}` is the display name. */
export function modelsListFailed(connectorName: string): string {
  return `Couldn't get ${connectorName}'s models.`;
}

/** The CLI's string, or the not-reported placeholder. Never a saved-picker guess. */
export function reportedModelLabel(model: string | null | undefined): string {
  const trimmed = model?.trim() ?? '';
  return trimmed.length > 0 ? trimmed : MODEL_NOT_REPORTED;
}
