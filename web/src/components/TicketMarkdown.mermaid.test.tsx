import '@testing-library/jest-dom/vitest';
import { beforeAll, describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { TicketMarkdown } from './TicketMarkdown';

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

const BROKEN_DIAGRAM = 'flowchart LR\n  A -->';
const DRAWN_DIAGRAM = 'flowchart LR\n  Plan --> Review';

describe('TicketMarkdown mermaid', () => {
  it('shows the diagram source when a diagram cannot be drawn', async () => {
    render(
      <TicketMarkdown>
        {`See the flow.\n\n\`\`\`mermaid\n${BROKEN_DIAGRAM}\n\`\`\`\n`}
      </TicketMarkdown>,
    );

    const hint = await screen.findByText(
      "This diagram couldn't be drawn, so here's its source.",
    );
    expect(hint.closest('pre')).toBeNull();
    const code = screen.getByText('A -->', { exact: false });
    expect(code.tagName).toBe('CODE');
    expect(code.closest('pre')).not.toBeNull();
    expect(
      hint.compareDocumentPosition(code) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(document.querySelector('svg')).toBeNull();
  });

  it('draws a mermaid block as a diagram', async () => {
    render(
      <TicketMarkdown>
        {`The plan.\n\n\`\`\`mermaid\n${DRAWN_DIAGRAM}\n\`\`\`\n`}
      </TicketMarkdown>,
    );

    const diagram = await screen.findByText('Plan');
    const svg = diagram.closest('svg');
    expect(document.querySelectorAll('svg')).toHaveLength(1);
    expect(svg).not.toBeNull();
    expect(svg?.querySelector('foreignObject')).toBeNull();
    expect(svg?.querySelector('script')).toBeNull();
    expect(svg).toHaveTextContent('Review');
    expect(
      screen.queryByText(
        "This diagram couldn't be drawn, so here's its source.",
      ),
    ).not.toBeInTheDocument();
  });
});
