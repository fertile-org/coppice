import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import type { ReactNode } from 'react';
import { describe, expect, it } from 'vitest';
import { ConfirmProvider, useConfirm } from './confirm-dialog';

function Probe({
  title = 'Remove plugin directory "/tmp"?',
  description,
  danger,
}: {
  title?: string;
  description?: string;
  danger?: boolean;
}) {
  const confirm = useConfirm();
  return (
    <button
      type="button"
      onClick={() => {
        void confirm({ title, description, danger }).then((ok) => {
          document.body.dataset.confirmResult = String(ok);
        });
      }}
    >
      Ask
    </button>
  );
}

function renderConfirm(ui: ReactNode = <Probe />) {
  return render(<ConfirmProvider>{ui}</ConfirmProvider>);
}

describe('useConfirm', () => {
  it('shows the wording and resolves true when Confirm is clicked', async () => {
    renderConfirm();

    fireEvent.click(screen.getByRole('button', { name: 'Ask' }));
    const dialog = screen.getByRole('dialog', {
      name: 'Remove plugin directory "/tmp"?',
    });
    expect(dialog).toBeVisible();

    fireEvent.click(within(dialog).getByRole('button', { name: 'Confirm' }));
    await waitFor(() => expect(document.body.dataset.confirmResult).toBe('true'));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('resolves false when Cancel is clicked', async () => {
    renderConfirm();

    fireEvent.click(screen.getByRole('button', { name: 'Ask' }));
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(document.body.dataset.confirmResult).toBe('false'));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('uses the danger confirm style when requested', () => {
    renderConfirm(<Probe danger />);

    fireEvent.click(screen.getByRole('button', { name: 'Ask' }));
    expect(screen.getByRole('button', { name: 'Confirm' })).toHaveClass(
      'text-danger',
    );
  });
});
