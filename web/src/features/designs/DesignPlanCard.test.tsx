import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { DESIGN_COPY } from './designCopy';
import { DesignPlanSummary, pickerDomId } from './DesignPlanCard';
import { makeDesignSet, makeOption, makePick } from './designFixtures';

describe('DesignPlanSummary', () => {
  it('renders nothing with 0 live plan-run sets', () => {
    const { container } = render(
      <DesignPlanSummary sets={[makeDesignSet({ superseded: true })]} />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it('summarizes one set as not picked', () => {
    render(
      <DesignPlanSummary
        sets={[makeDesignSet({ chosen: null })]}
        planMarkdown={'## UI design\n\n{{design:11111111-1111-4111-8111-111111111111}}\n'}
      />,
    );
    expect(screen.getByTestId('design-plan-summary')).toHaveTextContent(
      '1 decision in this plan',
    );
    expect(screen.getByRole('button', { name: /UI design: not picked/ })).toBeInTheDocument();
  });

  it('summarizes two sets with picks and scrolls to the picker', () => {
    const secondId = '22222222-2222-4222-8222-222222222222';
    const picked = makePick(makeOption('b', 'Full-width', 'B'));
    const target = document.createElement('div');
    target.id = pickerDomId(secondId);
    const scroll = vi.fn();
    target.scrollIntoView = scroll;
    document.body.appendChild(target);

    render(
      <DesignPlanSummary
        sets={[
          makeDesignSet({ chosen: picked }),
          makeDesignSet({
            id: secondId,
            title: 'Layout',
            chosen: null,
            options: [makeOption('a', 'Above', 'A')],
            recommended: makePick(makeOption('a', 'Above', 'A')),
            selected: makePick(makeOption('a', 'Above', 'A')),
          }),
        ]}
        sections={{
          '11111111-1111-4111-8111-111111111111': 'UI design',
          [secondId]: 'Layout',
        }}
      />,
    );
    expect(screen.getByTestId('design-plan-summary')).toHaveTextContent(
      '2 decisions in this plan',
    );
    expect(
      screen.getByRole('button', {
        name: DESIGN_COPY.sectionPick('UI design', DESIGN_COPY.optionName(picked)),
      }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /Layout: not picked/ }));
    expect(scroll).toHaveBeenCalled();
    target.remove();
  });
});
