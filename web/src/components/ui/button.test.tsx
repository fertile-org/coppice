import '@testing-library/jest-dom/vitest';
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { Button } from './button';

describe('Button', () => {
  it('styles an enabled button differently from a disabled one and keeps title', () => {
    const { rerender } = render(<Button>Run Agent</Button>);
    const enabled = screen.getByRole('button', { name: 'Run Agent' });
    expect(enabled.className).toContain('bg-btn-primary');
    expect(enabled.className).not.toContain('border-dashed');

    rerender(
      <Button disabled title="Repository path is not ready">
        Run Agent
      </Button>,
    );
    const disabled = screen.getByRole('button', { name: 'Run Agent' });
    expect(disabled.className).toContain('border-dashed');
    expect(disabled.className).toContain('bg-transparent');
    expect(disabled.className).toContain('text-btn-disabled-fg');
    expect(disabled.className).not.toContain('bg-btn-primary');
    expect(disabled.className).not.toContain('text-btn-primary-fg');
    expect(disabled.className).not.toContain('pointer-events-none');
    expect(disabled).toHaveAttribute('title', 'Repository path is not ready');
    expect(disabled).toBeDisabled();
  });
});
