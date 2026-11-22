import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import type { ComponentProps } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../lib/api';
import { selectComboboxOption } from '../../test/combobox';
import { NewTicketDialog } from './NewTicketDialog';

const REPOS = [
  { id: 'repo-1', name: 'coppice', verificationStatus: 'ready' },
];

function renderDialog(
  overrides: Partial<ComponentProps<typeof NewTicketDialog>> = {},
) {
  const onClose = vi.fn();
  const onCreate = vi.fn().mockResolvedValue({ id: 'ticket-new' });
  render(
    <NewTicketDialog
      open
      repos={REPOS}
      isPending={false}
      onClose={onClose}
      onCreate={onCreate}
      {...overrides}
    />,
  );
  return { onClose, onCreate };
}

describe('NewTicketDialog', () => {
  afterEach(() => {
    cleanup();
  });

  it('autofocuses the title field when opened', () => {
    renderDialog();
    expect(screen.getByLabelText('Title')).toHaveFocus();
  });

  it('submits title, description, repo and priority', async () => {
    const { onCreate, onClose } = renderDialog();
    const dialog = within(screen.getByRole('dialog', { name: 'New ticket' }));

    fireEvent.change(dialog.getByLabelText('Title'), {
      target: { value: ' Ship the dialog ' },
    });
    fireEvent.change(dialog.getByLabelText('Description'), {
      target: { value: '## Acceptance criteria\n- works' },
    });
    selectComboboxOption(dialog.getByLabelText('Repository'), 'coppice');
    selectComboboxOption(dialog.getByLabelText('Priority'), 'High');
    fireEvent.click(dialog.getByRole('button', { name: 'Create ticket' }));

    await waitFor(() => {
      expect(onCreate).toHaveBeenCalledWith({
        title: 'Ship the dialog',
        description: '## Acceptance criteria\n- works',
        repoId: 'repo-1',
        priority: 'high',
      });
    });
    expect(onClose).toHaveBeenCalled();
  });

  it('blocks empty or whitespace-only titles', () => {
    const { onCreate } = renderDialog();
    const dialog = within(screen.getByRole('dialog', { name: 'New ticket' }));
    const create = dialog.getByRole('button', { name: 'Create ticket' });

    expect(create).toBeDisabled();
    fireEvent.change(dialog.getByLabelText('Title'), { target: { value: '   ' } });
    expect(create).toBeDisabled();
    fireEvent.click(create);
    expect(onCreate).not.toHaveBeenCalled();
  });

  it('disables Create while the request is pending', () => {
    const { onCreate } = renderDialog({ isPending: true });
    const dialog = within(screen.getByRole('dialog', { name: 'New ticket' }));
    const create = dialog.getByRole('button', { name: 'Creating…' });
    expect(create).toBeDisabled();
    fireEvent.click(create);
    expect(onCreate).not.toHaveBeenCalled();
  });

  it('closes on Cancel without creating', () => {
    const { onCreate, onClose } = renderDialog();
    fireEvent.click(
      within(screen.getByRole('dialog', { name: 'New ticket' })).getByRole(
        'button',
        { name: 'Cancel' },
      ),
    );
    expect(onClose).toHaveBeenCalled();
    expect(onCreate).not.toHaveBeenCalled();
  });

  it('submits with Ctrl/⌘+Enter', async () => {
    const { onCreate } = renderDialog();
    const dialog = within(screen.getByRole('dialog', { name: 'New ticket' }));
    fireEvent.change(dialog.getByLabelText('Title'), {
      target: { value: 'From shortcut' },
    });
    fireEvent.keyDown(dialog.getByLabelText('Title'), {
      key: 'Enter',
      ctrlKey: true,
    });
    await waitFor(() => {
      expect(onCreate).toHaveBeenCalledWith(
        expect.objectContaining({ title: 'From shortcut', priority: 'medium' }),
      );
    });
  });

  it('ignores a second Mod+Enter while create is in flight', async () => {
    let resolveCreate: ((value: unknown) => void) | undefined;
    const onCreate = vi.fn(
      () =>
        new Promise((resolve) => {
          resolveCreate = resolve;
        }),
    );
    const { onClose } = renderDialog({ onCreate });
    const dialog = within(screen.getByRole('dialog', { name: 'New ticket' }));
    const title = dialog.getByLabelText('Title');

    fireEvent.change(title, { target: { value: 'Once only' } });
    fireEvent.keyDown(title, { key: 'Enter', ctrlKey: true });
    fireEvent.keyDown(title, { key: 'Enter', ctrlKey: true });

    expect(onCreate).toHaveBeenCalledTimes(1);
    resolveCreate?.({ id: 'ticket-new' });
    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
  });

  it('keeps the form and shows an inline error when create fails', async () => {
    const onCreate = vi
      .fn()
      .mockRejectedValue(new ApiError(400, '{"message":"Title is required"}'));
    renderDialog({ onCreate });
    const dialog = within(screen.getByRole('dialog', { name: 'New ticket' }));

    fireEvent.change(dialog.getByLabelText('Title'), {
      target: { value: 'Keep me' },
    });
    fireEvent.change(dialog.getByLabelText('Description'), {
      target: { value: 'Draft body' },
    });
    fireEvent.click(dialog.getByRole('button', { name: 'Create ticket' }));

    await waitFor(() => {
      expect(dialog.getByRole('alert')).toHaveTextContent('Title is required');
    });
    expect(dialog.getByLabelText('Title')).toHaveValue('Keep me');
    expect(dialog.getByLabelText('Description')).toHaveValue('Draft body');
    expect(screen.getByRole('dialog', { name: 'New ticket' })).toBeInTheDocument();
  });
});
