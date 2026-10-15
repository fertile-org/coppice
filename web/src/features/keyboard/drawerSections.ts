export const DRAWER_SECTIONS = ['detail', 'live', 'runs'] as const;

export type DrawerSection = (typeof DRAWER_SECTIONS)[number];

export const DRAWER_SECTION_LABELS: Record<DrawerSection, string> = {
  detail: 'Detail',
  live: 'Live Console',
  runs: 'Agent Runs',
};

/** Moves across Detail, Live Console, and Agent Runs. Does not wrap. */
export function stepDrawerSection(
  current: DrawerSection,
  direction: 1 | -1,
): DrawerSection {
  const index = DRAWER_SECTIONS.indexOf(current);
  const next = index + direction;
  if (next < 0 || next >= DRAWER_SECTIONS.length) return current;
  return DRAWER_SECTIONS[next];
}
