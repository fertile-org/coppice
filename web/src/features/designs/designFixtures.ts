import type { ChosenDesignOption, DesignOption, DesignSet } from '../../lib/schemas/design';

/** An option as the server returns it; the display title defaults to the title. */
export function makeOption(
  id: string,
  title: string,
  label: string,
  displayTitle: string = title,
): DesignOption {
  return { id, title, entry: `${id}/index.html`, label, displayTitle };
}

/** The `chosen`, `recommended` and `selected` shape. */
export function makePick(option: DesignOption): ChosenDesignOption {
  const { id, title, label, displayTitle } = option;
  return { id, title, label, displayTitle };
}

const quiet = makeOption('a', 'Quiet button after the last card', 'A');

/** A published set with three options; override fields per test. */
export function makeDesignSet(overrides: Partial<DesignSet> = {}): DesignSet {
  return {
    id: '11111111-1111-4111-8111-111111111111',
    title: 'Landing page',
    runId: '22222222-2222-4222-8222-222222222222',
    options: [
      quiet,
      makeOption('b', 'Full-width button with Show all', 'B'),
      makeOption('c', 'Footer pinned to the viewport', 'C'),
    ],
    chosen: null,
    recommended: makePick(quiet),
    selected: makePick(quiet),
    superseded: false,
    fromPlanRun: true,
    inherited: false,
    choice: { mode: 'update_plan', planOutOfDate: false },
    ...overrides,
  };
}
