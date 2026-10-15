import '@testing-library/jest-dom/vitest';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';
import { ToastProvider } from '../../components/ToastProvider';
import { KeyboardProvider } from '../keyboard/KeyboardProvider';
import { TicketCommentsTab } from './TicketCommentsTab';

const createComment = vi.hoisted(() => vi.fn());

vi.mock('../agents/useAgents', () => ({
  useAgents: () => ({
    data: [
      {
        id: '00000000-0000-0000-0000-000000000001',
        name: 'PM Codex',
        role: 'PM',
        enabled: true,
        presetSource: 'pm',
      },
      {
        id: '00000000-0000-0000-0000-000000000002',
        name: 'PM Opencode',
        role: 'PM',
        enabled: true,
        presetSource: 'pm',
      },
    ],
  }),
}));

vi.mock('./useTicket', () => ({
  useComments: () => ({ data: [], isLoading: false, isError: false }),
  useCreateComment: () => ({ mutateAsync: createComment, isPending: false }),
  useUploadAttachment: () => ({ mutateAsync: vi.fn(), isPending: false }),
}));

function renderComments() {
  const client = new QueryClient();
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <KeyboardProvider>
          <ToastProvider>
            <TicketCommentsTab ticketId="00000000-0000-0000-0000-000000000003" />
          </ToastProvider>
        </KeyboardProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('TicketCommentsTab', () => {
  beforeEach(() => {
    createComment.mockReset();
    createComment.mockResolvedValue({ startedRuns: [] });
  });

  it('suggests distinct mentions by full agent name when presets match', () => {
    renderComments();

    fireEvent.click(screen.getByRole('button', { name: 'Markdown source' }));
    const textarea = screen.getByPlaceholderText('Write a comment in markdown…');
    fireEvent.change(textarea, { target: { value: '@pm' } });

    const listbox = screen.getByRole('listbox', { name: 'Agent mentions' });
    expect(within(listbox).getByText('PM Codex')).toBeInTheDocument();
    expect(within(listbox).getByText('@pm-codex')).toBeInTheDocument();
    expect(within(listbox).getByText('PM Opencode')).toBeInTheDocument();
    expect(within(listbox).getByText('@pm-opencode')).toBeInTheDocument();
    expect(within(listbox).queryAllByText('@pm')).toHaveLength(0);
  });

  it('completes a mention into the comment body', () => {
    renderComments();

    fireEvent.click(screen.getByRole('button', { name: 'Markdown source' }));
    const textarea = screen.getByPlaceholderText<HTMLTextAreaElement>(
      'Write a comment in markdown…',
    );
    fireEvent.change(textarea, { target: { value: 'hi @pm' } });
    const listbox = screen.getByRole('listbox', { name: 'Agent mentions' });
    fireEvent.click(within(listbox).getByText('PM Codex'));

    expect(textarea.value).toBe('hi @pm-codex ');
    expect(
      screen.queryByRole('listbox', { name: 'Agent mentions' }),
    ).not.toBeInTheDocument();
  });

  it('renders the rich-text comment editor by default', () => {
    renderComments();

    expect(screen.getByRole('textbox', { name: 'Comment' })).toHaveAttribute(
      'contenteditable',
      'true',
    );
    expect(screen.getByRole('button', { name: 'Post comment' })).toBeDisabled();
  });

  it('posts once with Mod+Enter from the comment editor and stays quiet otherwise', async () => {
    renderComments();

    fireEvent.click(screen.getByRole('button', { name: 'Markdown source' }));
    const textarea = screen.getByPlaceholderText('Write a comment in markdown…');
    textarea.focus();
    fireEvent.keyDown(textarea, { key: 'Enter', ctrlKey: true });
    expect(createComment).not.toHaveBeenCalled();

    fireEvent.change(textarea, { target: { value: 'Ship the gate' } });
    const elsewhere = document.createElement('input');
    document.body.appendChild(elsewhere);
    elsewhere.focus();
    fireEvent.keyDown(elsewhere, { key: 'Enter', ctrlKey: true });
    expect(createComment).not.toHaveBeenCalled();
    elsewhere.remove();

    textarea.focus();
    fireEvent.keyDown(textarea, { key: 'Enter', ctrlKey: true });
    await waitFor(() => {
      expect(createComment).toHaveBeenCalledTimes(1);
    });
    expect(createComment).toHaveBeenCalledWith(
      expect.objectContaining({ body: 'Ship the gate' }),
    );
  });
});
