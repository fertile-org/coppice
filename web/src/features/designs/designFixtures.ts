import type {
  ChosenDesignOption,
  DesignOption,
  DesignSet,
} from '../../lib/schemas/design';

/** An option as the server returns it; the display title defaults to the title. */
export function makeOption(
  id: string,
  title: string,
  label: string,
  displayTitle: string = title,
): DesignOption {
  return { id, title, entry: `${id}/index.html`, label, displayTitle };
}

/** Structured technical option (no HTML entry). */
export function makeTechnicalOption(
  id: string,
  title: string,
  label: string,
  fields: Partial<
    Pick<
      DesignOption,
      'changes' | 'effort' | 'risk' | 'reversibility' | 'rulesOut' | 'sketch'
    >
  > = {},
): DesignOption {
  return {
    id,
    title,
    entry: '',
    label,
    displayTitle: title,
    changes: fields.changes ?? 'server/src/example.rs',
    effort: fields.effort ?? 'S',
    risk: fields.risk ?? 'Low',
    reversibility: fields.reversibility ?? 'Easy',
    rulesOut: fields.rulesOut ?? 'Nothing soon',
    sketch: fields.sketch ?? 'flowchart LR\n  A --> B',
  };
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
    kind: 'ui',
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
    fromChat: false,
    inherited: false,
    choice: { mode: 'update_plan', planOutOfDate: false },
    ...overrides,
  };
}

/** A technical set with two structured options. */
export function makeTechnicalDesignSet(
  kind: 'architecture' | 'module' | 'dependency',
  overrides: Partial<DesignSet> = {},
): DesignSet {
  const a = makeTechnicalOption('a', 'Postgres LISTEN/NOTIFY', 'A');
  const b = makeTechnicalOption('b', 'Embedded message queue', 'B', {
    effort: 'M',
    risk: 'Medium',
    reversibility: 'Hard',
    sketch: 'flowchart LR\n  Service --> Queue --> Hub',
  });
  return makeDesignSet({
    title: `Technical decision (${kind})`,
    kind,
    options: [a, b],
    recommended: makePick(a),
    selected: makePick(a),
    recommendedReason: 'No new dependency; fits bundled Postgres.',
    ...overrides,
  });
}
