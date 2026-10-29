/** User-facing plugin strings. Content owns this list. */
export const PLUGIN_DOCS_URL = 'https://getcoppice.app/docs/plugins';
export const PLUGIN_FORMAT_DOCS_URL = `${PLUGIN_DOCS_URL}#create-a-plugin`;

export const PLUGIN_COPY = {
  pageIntro: 'Give your agents new skills and tools.',
  whichAgents: 'Which agents can use it?',
  allAgents: 'All agents',
  chooseAgents: 'Choose agents',
  noAgents: 'No agents',
  oneAgent: '1 agent',
  loadingAgents: 'Loading agents…',
  noAgentsYet: 'No agents yet.',
  loadAgentsFailed: "Couldn't load agents. Try again.",
  updateAgentsFailed: "Couldn't update which agents can use it. Try again.",
  nextRun: "Changes apply from each agent's next run.",
  guideChooseTitle: 'Choose',
  guideChooseBefore: 'pick All agents, or check specific agents here or in',
  guideChooseLink: 'Agents',
  marketplaceTitle: 'Coppice marketplace',
  marketplaceIntro:
    'Plugins you can install with one click. Once installed, a plugin is available to all agents you give access to, whatever CLI they run on.',
  trustCoppice: 'Coppice',
  trustCommunity: 'Community: not reviewed by Coppice',
  marketplaceEmpty: 'No plugins match these filters.',
  marketplaceNotInstallable: "Can't install from Coppice yet.",
  installed: 'Installed',
  updateAvailable: 'Update available',
  installing: 'Installing…',
  installedNotice: (name: string) =>
    `Installed ${name}. Enable it and choose which agents can use it.`,
  updatedNotice: (name: string) => `Updated ${name}. Changes apply from each agent's next run.`,
  sourcesTitle: 'Sources',
  sourcesIntro:
    'Coppice is always listed. Add a git repository that has a .claude-plugin/marketplace.json to list its plugins too.',
} as const;

export interface PluginSettingHelp {
  label: string;
  help: string;
  scopes?: string;
  tokenUrl?: string;
  tokenLinkLabel?: string;
}

/** Guidance for known setting keys. Unknown keys show only the raw key. */
export const PLUGIN_SETTING_HELP: Record<string, PluginSettingHelp> = {
  GITHUB_PERSONAL_ACCESS_TOKEN: {
    label: 'GitHub access token',
    help: 'Lets the github plugin read and update issues, pull requests and CI for your agents. Stored encrypted; Coppice never shows it again.',
    scopes:
      'Required: read access to Metadata, Contents, Issues, Pull requests and Actions. Add write access to Issues and Pull requests only if agents should comment or edit.',
    tokenUrl: 'https://github.com/settings/personal-access-tokens/new',
    tokenLinkLabel: 'Create a token on GitHub',
  },
};

export function missingSettingsNotice(keys: string[]): string {
  return keys.length === 1
    ? `Add ${keys[0]} in its Settings before agents can use it.`
    : `Add ${keys.join(', ')} in its Settings before agents can use it.`;
}

/** `{n} agents` for counts other than 0 and 1. */
export function pluginAgentCount(count: number): string {
  return `${count} agents`;
}

export function pluginAgentSummary(
  access: 'all' | 'explicit',
  agentCount: number,
): string {
  if (access === 'all') return PLUGIN_COPY.allAgents;
  if (agentCount === 0) return PLUGIN_COPY.noAgents;
  if (agentCount === 1) return PLUGIN_COPY.oneAgent;
  return pluginAgentCount(agentCount);
}
