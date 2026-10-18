/** User-facing init wizard strings. Content owns this list. */

export const INIT_COPY = {
  welcomeTitle: 'Welcome to Coppice',
  welcomeIntro:
    'This short setup gets you started. Connect a repository, create your first board, and add a starter team of agents. It takes about a minute, and you can skip it.',
  title: 'Set up Coppice',
  intro:
    'Pick a repository, name a board, and add a starter team. Nothing is created until you click Create.',
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
  notGit: "Choose a folder that's a git repository.",
  pathMissing: 'Path missing',
  notGitRepo: 'Not a git repo',
  pathError: 'Error',
  pathReady: 'Ready',
  inspectFailed: "Couldn't read that folder.",
  boardNameRequired: 'Enter a board name.',
  teamHeading: 'Starter team',
  agentCli: 'Agent CLI',
  teamConnector: 'They run on {connector} with its default model.',
  teamIdle: 'Nothing runs until you click Run agent on a ticket.',
  checking: 'Checking installed CLIs…',
  noReady:
    'No agent CLI is ready yet. Install one and sign in, then check it in System → Connectors.',
  loadFailed: "Couldn't load setup.",
  loading: 'Loading setup…',
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
