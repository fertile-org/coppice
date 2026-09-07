import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { ToolShell } from './ToolShell';

describe('ToolShell', () => {
  it('shows error output without requiring expand click', () => {
    render(
      <ToolShell status="error" title="cargo test" variant="shell">
        exit code 1
      </ToolShell>,
    );

    expect(screen.getByText('exit code 1')).toBeVisible();
  });

  it('keeps completed output collapsed until clicked', () => {
    render(
      <ToolShell status="completed" title="cargo test" variant="shell">
        all passed
      </ToolShell>,
    );

    expect(screen.queryByText('all passed')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button'));

    expect(screen.getByText('all passed')).toBeVisible();
  });

  it('opens output when a running row becomes error', () => {
    const { rerender } = render(
      <ToolShell status="running" title="cargo test" variant="shell" />,
    );

    expect(screen.queryByText('exit code 1')).not.toBeInTheDocument();

    rerender(
      <ToolShell status="error" title="cargo test" variant="shell">
        exit code 1
      </ToolShell>,
    );

    expect(screen.getByText('exit code 1')).toBeVisible();
  });
});
