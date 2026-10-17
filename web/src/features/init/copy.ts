/** User-facing init wizard strings. Content owns this list. */

export const INIT_COPY = {
  title: 'Set up Coppice',
  intro:
    'Pick a repository, name a board, and add a starter team. Nothing is created until you confirm.',
  stepOf: 'Step {current} of 3',
  repository: 'Repository',
  board: 'Board',
  team: 'Team',
  localPath: 'Local path',
  browse: 'Browse…',
  name: 'Name',
  remoteUrl: 'Remote URL',
  remoteOptional: '(optional)',
  defaultBranch: 'Default branch',
  pathRequired: 'Enter a local path.',
  notGit: 'Pick a git checkout before continuing.',
  pathMissing: 'Path missing',
  notGitRepo: 'Not a git repo',
  pathError: 'Error',
  pathReady: 'Ready',
  inspectFailed: "Couldn't read that folder.",
  boardNameRequired: 'Enter a board name.',
  teamHeading: 'Starter team',
  teamConnector: "They use {connector}'s default model.",
  teamIdle: 'Nothing runs until you click Run agent on a ticket.',
  checking: 'Checking installed CLIs…',
  noReady:
    'No agent CLI is ready yet. Install one and sign in, then run a check on Tools → Connectors.',
  createWaits: 'Create stays off until a CLI shows Ready.',
  loadFailed: "Couldn't load setup.",
  loading: 'Loading setup…',
  duplicatePath: 'That folder is already registered.',
  duplicateBoard: 'A board with that name already exists.',
  saveFailed: "Couldn't create the workspace. Nothing was saved.",
  back: 'Back',
  next: 'Next',
  skip: 'Skip',
  create: 'Create',
  creating: 'Creating…',
  toolsButton: 'Set up workspace',
} as const;

export function stepLabel(current: number): string {
  return INIT_COPY.stepOf.replaceAll('{current}', String(current));
}

export function teamConnectorLine(connectorName: string): string {
  return INIT_COPY.teamConnector.replaceAll('{connector}', connectorName);
}

export function joinNames(names: string[]): string {
  if (names.length <= 1) return names[0] ?? '';
  if (names.length === 2) return `${names[0]} and ${names[1]}`;
  return `${names.slice(0, -1).join(', ')}, and ${names[names.length - 1]}`;
}
