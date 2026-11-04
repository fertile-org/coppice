/** User-facing agent strings. Content owns this list. */
export const AGENT_COPY = {
  skillsHeading: 'Skills',
  skillsHint: '(plugins this agent can use)',
  thinkingLevelLabel: 'Thinking level',
  thinkingLevelNames: { low: 'Low', medium: 'Medium', high: 'High', max: 'Max' },
  /** Agents table, after the model: `{level}` is the stored level. */
  thinkingLevelInTable: '{level} thinking',
} as const;
