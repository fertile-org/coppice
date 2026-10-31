/** User-facing design-option strings. Content owns this list. */
export const DESIGN_COPY = {
  cardTitle: 'Click to open design options',
  open: 'Open',
  dialogTitle: 'Design options',
  recommended: 'Recommended',
  chosen: 'Chosen',
  noPreview: 'This preview has no file.',
  loadingPreview: 'Loading preview…',
  options: (n: number) => `${n} ${n === 1 ? 'option' : 'options'}`,
  using: (id: string, title: string, yourChoice: boolean) =>
    `Using ${id.toUpperCase()}: ${title} (${yourChoice ? 'your choice' : 'recommended'})`,
  workFollows: 'Work follows this option.',
  planNotUpdated: "The plan wasn't updated to this option.",
  tryAgain: 'Try again',
  planRunning: 'The plan is being updated.',
  planFinal:
    'The plan is approved, so this design is final. To change it, add a comment on the ticket.',
  choose: (id: string) => `Choose ${id.toUpperCase()}`,
  chooseAndUpdate: (id: string) => `Choose ${id.toUpperCase()} and update plan`,
  saveFailed: 'Unable to save that choice.',
} as const;
