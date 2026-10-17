import '@testing-library/jest-dom/vitest';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';
import { ToastProvider } from '../../components/ToastProvider';
import { KeyboardProvider } from '../keyboard/KeyboardProvider';
import { TicketCommentsTab } from './TicketCommentsTab';

const createComment = vi.hoisted(() => vi.fn());
const commentPages = vi.hoisted(() => ({
  newest: [] as unknown[],
  fetchOlder: vi.fn(),
}));

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
  COMMENTS_PAGE_SIZE: 200,
  fetchOlderComments: (...args: unknown[]) => commentPages.fetchOlder(...args),
  useComments: () => ({ data: commentPages.newest, isLoading: false, isError: false }),
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
    commentPages.newest = [];
    commentPages.fetchOlder.mockReset();
    createComment.mockReset();
    createComment.mockResolvedValue({ startedRuns: [] });
  });

  it('adds a pending chip when a file is dropped on the comment box', () => {
    vi.stubGlobal('URL', {
      ...URL,
      createObjectURL: () => 'blob:shot',
      revokeObjectURL: () => {},
    });
    renderComments();
    const form = screen.getByLabelText('Comment').closest('form');
    expect(form).not.toBeNull();
    const shot = new File(['png'], 'shot.png', { type: 'image/png' });
    fireEvent.dragOver(form!, { dataTransfer: { types: ['Files'], files: [shot] } });
    expect(screen.getByText('Drop to add to your comment')).toBeInTheDocument();
    fireEvent.drop(form!, { dataTransfer: { types: ['Files'], files: [shot] } });
    expect(screen.getByAltText('shot.png')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Attach files' })).toBeInTheDocument();
  });

  it('adds a pending chip when an image is pasted into the comment box', () => {
    vi.stubGlobal('URL', {
      ...URL,
      createObjectURL: () => 'blob:pasted',
      revokeObjectURL: () => {},
    });
    renderComments();
    const form = screen.getByLabelText('Comment').closest('form');
    const shot = new File(['png'], 'pasted.png', { type: 'image/png' });
    fireEvent.paste(form!, {
      clipboardData: {
        items: [{ kind: 'file', type: 'image/png', getAsFile: () => shot }],
      },
    });
    expect(screen.getByAltText('pasted.png')).toBeInTheDocument();
  });

  it('toasts the locked type and size sentences and skips the chip', () => {
    renderComments();
    const form = screen.getByLabelText('Comment').closest('form');
    const html = new File(['<p>'], 'page.html', { type: 'text/html' });
    const huge = new File([new Uint8Array(10 * 1024 * 1024 + 1)], 'diagram.png', {
      type: 'image/png',
    });
    fireEvent.drop(form!, { dataTransfer: { types: ['Files'], files: [html, huge] } });
    expect(screen.getByText("Coppice can't attach html files.")).toBeInTheDocument();
    expect(
      screen.getByText('diagram.png is too large. The limit is 10.0 MB.'),
    ).toBeInTheDocument();
    expect(screen.queryByAltText('diagram.png')).not.toBeInTheDocument();
    expect(screen.queryByText('page.html')).not.toBeInTheDocument();
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

  it('loads older comments after a full first page', async () => {
    const comment = (n: number) => ({
      id: `00000000-0000-0000-0001-${String(n).padStart(12, '0')}`,
      ticketId: '00000000-0000-0000-0000-000000000003',
      authorType: 'human',
      body: `Comment ${n}`,
      intent: 'progress_update',
      mentions: [],
      attachmentIds: [],
      attachments: [],
      createdAt: '2026-10-18T00:00:00Z',
    });
    commentPages.newest = Array.from({ length: 200 }, (_, i) => comment(400 - i));
    commentPages.fetchOlder.mockResolvedValue({ comments: [comment(200), comment(199)], nextBefore: null });
    renderComments();

    fireEvent.click(screen.getByRole('button', { name: 'Load older comments' }));
    expect(await screen.findByText('Comment 199')).toBeInTheDocument();
    expect(commentPages.fetchOlder).toHaveBeenCalledWith(
      '00000000-0000-0000-0000-000000000003',
      '00000000-0000-0000-0001-000000000201',
    );
    expect(screen.queryByRole('button', { name: 'Load older comments' })).not.toBeInTheDocument();
  });

  it('has no Load older button when the first page is short', () => {
    commentPages.newest = [];
    renderComments();
    expect(screen.queryByRole('button', { name: 'Load older comments' })).not.toBeInTheDocument();
  });
});
