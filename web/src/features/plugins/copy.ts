/** User-facing plugin strings. Content owns this list. */
export const PLUGIN_COPY = {
  pageIntro:
    'Plugins add skills and tools to your agents. Add a plugin, enable it, then choose which agents can use it.',
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
} as const;

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
