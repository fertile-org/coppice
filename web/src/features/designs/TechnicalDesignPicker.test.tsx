import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, within } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { DESIGN_COPY } from './designCopy';
import { makeTechnicalDesignSet, makeTechnicalOption, makePick } from './designFixtures';
import { pickerDomId } from './DesignPlanCard';
import { DesignPlanSummary } from './DesignPlanCard';
import { TechnicalDesignPicker } from './TechnicalDesignPicker';

const apiFetch = vi.hoisted(() => vi.fn());

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch,
}));

beforeAll(() => {
  const svg = SVGElement.prototype as SVGElement & {
    getComputedTextLength?: () => number;
    getBBox?: () => DOMRect;
  };
  svg.getComputedTextLength ??= () => 24;
  svg.getBBox ??= () =>
    ({
      x: 0,
      y: 0,
      width: 48,
      height: 16,
      top: 0,
      right: 48,
      bottom: 16,
      left: 0,
      toJSON() {
        return {};
      },
    }) as DOMRect;
});

function wrap(node: ReactNode) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(<QueryClientProvider client={client}>{node}</QueryClientProvider>);
}

const kinds = ['architecture', 'module', 'dependency'] as const;

describe.each(kinds)('TechnicalDesignPicker (%s)', (kind) => {
  beforeEach(() => {
    apiFetch.mockReset();
    apiFetch.mockResolvedValue(new Response(null, { status: 204 }));
  });

  it('renders cards with diagram, fields, badges, and summary counts', async () => {
    const set = makeTechnicalDesignSet(kind, {
      chosen: makePick(
        makeTechnicalOption('a', 'Postgres LISTEN/NOTIFY', 'A'),
      ),
    });
    wrap(
      <>
        <DesignPlanSummary
          sets={[set]}
          sections={{ [set.id]: 'Technical decision' }}
        />
        <TechnicalDesignPicker
          design={set}
          ticketId="ticket-1"
          sectionId={pickerDomId(set.id)}
        />
      </>,
    );

    const summary = screen.getByTestId('design-plan-summary');
    expect(summary).toHaveTextContent('1 decision in this plan · 2 options');
    expect(
      screen.getByRole('button', {
        name: DESIGN_COPY.sectionPick(
          'Technical decision',
          DESIGN_COPY.optionName(set.chosen!),
        ),
      }),
    ).toBeInTheDocument();

    const picker = screen.getByTestId(`design-picker-${set.id}`);
    expect(picker).toHaveAttribute('data-design-kind', kind);

    const cardA = screen.getByTestId('technical-option-a');
    expect(within(cardA).getByText(DESIGN_COPY.recommended)).toBeInTheDocument();
    expect(within(cardA).getByText(DESIGN_COPY.yourPick)).toBeInTheDocument();
    expect(within(cardA).getByText(DESIGN_COPY.fieldChanges)).toBeInTheDocument();
    expect(within(cardA).getByText(DESIGN_COPY.fieldEffort)).toBeInTheDocument();
    expect(within(cardA).getByText(DESIGN_COPY.fieldRisk)).toBeInTheDocument();
    expect(within(cardA).getByText(DESIGN_COPY.fieldReversibility)).toBeInTheDocument();
    expect(within(cardA).getByText(DESIGN_COPY.fieldRulesOut)).toBeInTheDocument();
    expect(within(cardA).getByText(/Why:/)).toHaveTextContent(
      set.recommendedReason!,
    );

    await within(cardA).findByText('B', {}, { timeout: 5000 });
    expect(cardA.querySelector('svg')).not.toBeNull();
  });
});

describe('TechnicalDesignPicker broken sketch', () => {
  beforeEach(() => {
    apiFetch.mockReset();
    apiFetch.mockResolvedValue(new Response(null, { status: 204 }));
  });

  it('shows the sketch source when the diagram cannot be drawn', async () => {
    const broken = makeTechnicalOption('a', 'Broken', 'A', {
      sketch: 'flowchart LR\n  A -->',
    });
    const other = makeTechnicalOption('b', 'Other', 'B');
    const set = makeTechnicalDesignSet('architecture', {
      options: [broken, other],
      recommended: makePick(broken),
      selected: makePick(broken),
      recommendedReason: 'Best of a bad set.',
    });

    wrap(
      <TechnicalDesignPicker
        design={set}
        ticketId="ticket-1"
        sectionId={pickerDomId(set.id)}
      />,
    );

    const hint = await screen.findByText(
      "This diagram couldn't be drawn, so here's its source.",
    );
    expect(hint.closest('pre')).toBeNull();
    const code = screen.getByText('A -->', { exact: false });
    expect(code.tagName).toBe('CODE');
  });

  it('opens the full-size diagram dialog from expand', async () => {
    const set = makeTechnicalDesignSet('module');
    wrap(
      <TechnicalDesignPicker
        design={set}
        ticketId="ticket-1"
        sectionId={pickerDomId(set.id)}
      />,
    );

    fireEvent.click(screen.getAllByLabelText(DESIGN_COPY.expandDiagram)[0]!);
    expect(await screen.findByText(DESIGN_COPY.fullDiagram)).toBeInTheDocument();
    expect(
      screen.getAllByText(DESIGN_COPY.optionName(set.options[0]!)).length,
    ).toBeGreaterThan(1);
  });
});
