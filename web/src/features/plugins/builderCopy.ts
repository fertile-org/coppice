/** User-facing plugin builder strings. Content owns this list. */
export const BUILDER_COPY = {
  buildButton: 'Build a plugin',
  emptyBuildLink: 'build one',
  emptyBuildLine: 'Or describe what you need and let your agents',
  backToPlugins: '← Plugins',
  stepOf: (step: number, total: number) => `Step ${step} of ${total}`,
  next: 'Next',
  back: 'Back',
  seeThePlan: 'See the plan',
  leaveTitle: 'Leave the builder? Your answers will be lost.',
  leaveConfirm: 'Leave',
  leaveStay: 'Stay',
  writingPlan: 'Writing plan',
  approveStartsBuild: 'Approving starts the build. Nothing is turned on until you accept it.',
  advanced: 'Advanced',

  q1Title: 'What should this plugin help your agents with?',
  q1Help: 'One or two sentences is enough.',
  q1Placeholder: 'For example: keep our changelog up to date',

  q2Title: 'What should it do?',
  q2Help: 'List the jobs it should handle. One per line.',
  q2Placeholder: 'Read new commits\nWrite a short summary\nAdd it to CHANGELOG.md',

  q3Title: 'Which tools or apps should it work with?',
  q3Help: 'Pick any that apply, or type your own. Leave it as None if it only needs instructions.',
  q3Placeholder: 'For example: our deploy script, Jira, ffmpeg',
  q3None: 'None',
  q3Git: 'git',
  q3GitHub: 'GitHub',
  q3Cli: 'A command-line tool',
  q3Api: 'A web service or API',

  q4Title: 'Does it need a password, key or internet access?',
  q4Help: 'Coppice stores keys encrypted and never shows them again.',
  q4None: 'No, it works offline',
  q4Internet: 'Yes, internet only',
  q4Key: 'Yes, a key or password too',
  q4ServiceLabel: 'Which service?',
  q4ServicePlaceholder: 'For example: GitHub, our status page',

  q5Title: 'Where should it live?',
  q5Local: 'Just this computer',
  q5LocalHelp: 'You can publish it later.',
  q5Git: 'A git repository',
  q5GitHelp: 'Share it with your team.',
  q5RepoLabel: 'Repository URL',
  q5RepoPlaceholder: 'https://github.com/your-team/plugins.git',
  q5RepoHelp: 'Coppice adds the plugin there after you accept it.',

  // §6 progress
  buildingTitle: (name: string) => `Building ${name}`,
  stepPlanApproved: 'Plan approved',
  stepWriting: 'Writing the plugin',
  stepChecking: 'Checking it',
  stepInstalling: 'Installing it, turned off',
  stepTesting: 'Testing it',
  showDetails: 'Show details',
  stopBuild: 'Stop build',
  stopBuildTitle: 'Stop building? You can start again from the plan.',
  stopBuildConfirm: 'Stop',
  stopBuildKeep: 'Keep building',
  installedBuilding: (name: string) => `${name} · Building…`,
  installedReady: (name: string) => `${name} · Ready to try`,
  open: 'Open',
  review: 'Review',
  buildStoppedClosed: 'Build stopped when Coppice closed.',

  // §7 try / accept
  readyTitle: (name: string) => `${name} is ready to try`,
  readySub: "It's installed and turned off.",
  testPassed: 'Passed',
  testFailed: "Didn't pass",
  testAgain: 'Test again',
  acceptDisabledTitle: 'Run a test that passes first.',
  accept: 'Accept',
  discard: 'Discard',
  discardTitle: 'Discard this plugin? It will be removed from this computer.',
  discardConfirm: 'Discard',
  discardKeep: 'Keep',
  acceptDialogTitle: 'Accept this plugin?',
  acceptDialogBody: 'It stays off until you turn it on.',
  acceptEnable: 'Turn it on for all agents now',
  acceptPublish: (repo: string) => `Publish to ${repo}`,
  acceptCancel: 'Cancel',
  acceptedNotice: (name: string) => `Accepted ${name}.`,

  // §8 publish
  publishPushed: (repo: string, branch: string) =>
    `Pushed to ${repo} on branch ${branch}. Open a pull request there to share it.`,
  openRepository: 'Open repository ↗',
  publishLater: 'Publish…',
  publishDialogTitle: 'Publish this plugin?',
  publishDialogBody: 'Coppice adds it to the repository on a new branch.',
  publishUrlLabel: 'Repository URL',
  publishUrlPlaceholder: 'https://github.com/your-team/plugins.git',
  publishConfirm: 'Publish',
  publishCancel: 'Cancel',
  publishBranchExists: (branch: string) =>
    `A branch for this plugin is already there. Coppice will push to ${branch}.`,
  publishPushSuffix: 'Push',
  keepItLocal: 'Keep it local',

  // §10 errors
  buildDidNotFinish: "The build didn't finish.",
  checkFailed: "The plugin didn't pass Coppice's checks.",
  tryAgain: 'Try again',
  askForChanges: 'Ask for changes',
  publishUnreachable: (repo: string) =>
    `Couldn't push to ${repo}. Check that this computer can push there, then try again.`,
  publishInvalidUrl: "That doesn't look like a repository URL.",
} as const;

export const TOOL_CHIP_VALUES = {
  none: 'none',
  git: 'git',
  GitHub: 'GitHub',
  cli: 'cli',
  api: 'api',
} as const;

export type ToolChipValue = (typeof TOOL_CHIP_VALUES)[keyof typeof TOOL_CHIP_VALUES];
