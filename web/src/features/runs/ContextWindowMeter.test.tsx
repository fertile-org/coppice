import '@testing-library/jest-dom/vitest';
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { contextWindowFromEvent } from './contextWindow';
import { ContextWindowMeter } from './ContextWindowMeter';

describe('contextWindowFromEvent', () => {
  it('reads a context-window event and ignores everything else', () => {
    expect(
      contextWindowFromEvent({ type: 'usage.contextWindow', used: 84000, size: 200000 }),
    ).toEqual({ used: 84000, size: 200000 });
    expect(contextWindowFromEvent({ type: 'usage.contextWindow', used: 5, size: null })).toEqual({
      used: 5,
      size: null,
    });
    expect(contextWindowFromEvent({ type: 'claude-code.console.text', used: 5 })).toBeNull();
    expect(contextWindowFromEvent({ type: 'usage.contextWindow', used: 'x' })).toBeNull();
  });
});

describe('ContextWindowMeter', () => {
  it('exposes progress when size is known and omits the bar otherwise', () => {
    const { unmount } = render(<ContextWindowMeter used={84000} size={200000} />);
    const bar = screen.getByRole('progressbar', { name: 'Context window' });
    expect(bar).toHaveAttribute('aria-valuenow', '84000');
    expect(bar).toHaveAttribute('aria-valuemax', '200000');
    unmount();

    render(<ContextWindowMeter used={20919} size={null} />);
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
  });
});
