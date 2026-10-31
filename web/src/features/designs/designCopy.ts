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
  /** "A: Quiet button". The server supplies both parts. */
  optionName: (option: { label: string; displayTitle: string }) =>
    `${option.label}: ${option.displayTitle}`,
  using: (option: { label: string; displayTitle: string }, yourChoice: boolean) =>
    `Using ${option.label}: ${option.displayTitle} (${yourChoice ? 'your choice' : 'recommended'})`,
  workFollows: 'Work follows this option.',
  planNotUpdated: "The plan wasn't updated to this option.",
  tryAgain: 'Try again',
  planRunning: 'The plan is being updated.',
  planFinal:
    'The plan is approved, so this design is final. To change it, add a comment on the ticket.',
  choose: (label: string) => `Choose ${label}`,
  chooseAndUpdate: (label: string) => `Choose ${label} and update plan`,
  saveFailed: 'Unable to save that choice.',
} as const;
