import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { ToolShell } from './ToolShell';

describe('ToolShell', () => {
  it('shows error output immediately and when a run fails', () => {
    render(
      <ToolShell status="error" title="cargo test" variant="shell">
        exit code 1
      </ToolShell>,
    );
    expect(screen.getByText('exit code 1')).toBeVisible();
    cleanup();

    const { rerender } = render(
      <ToolShell status="running" title="cargo test" variant="shell" />,
    );
    rerender(
      <ToolShell status="error" title="cargo test" variant="shell">
        boom
      </ToolShell>,
    );
    expect(screen.getByText('boom')).toBeVisible();
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
});