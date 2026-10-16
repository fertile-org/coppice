import '@testing-library/jest-dom/vitest';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';
import { ToastProvider } from '../../components/ToastProvider';
import { KeyboardProvider } from '../keyboard/KeyboardProvider';
import type { Ticket } from '../board/useTickets';
import { TicketDetailPanel } from './TicketDetailPanel';

const upload = vi.hoisted(() => vi.fn());
const updateTicket = vi.hoisted(() => vi.fn());

vi.mock('../agents/useAgents', () => ({
  useAgents: () => ({ data: [] }),
}));

vi.mock('./useTicket', () => ({
  useTicketChildren: () => ({ data: [] }),
  useUpdateTicket: () => ({ mutateAsync: updateTicket, isPending: false }),
  useUploadAttachment: () => ({ mutateAsync: upload, isPending: false }),
  useComments: () => ({ data: [], isLoading: false, isError: false }),
  useCreateComment: () => ({ mutateAsync: vi.fn(), isPending: false }),
}));

const ticket: Ticket = {
  id: '00000000-0000-0000-0000-000000000001',
  boardId: '00000000-0000-0000-0000-000000000002',
  title: 'Test ticket',
  description: 'Ticket description',
  status: 'backlog',
  createdBy: 'user',
  createdAt: '2026-06-08T00:00:00.000Z',
  updatedAt: '2026-06-08T00:00:00.000Z',
  lastActivityAt: '2026-06-08T00:00:00.000Z',
  attachmentIds: [],
  attachments: [],
};

function renderPanel() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <KeyboardProvider>
          <ToastProvider>
            <TicketDetailPanel ticket={ticket} />
          </ToastProvider>
        </KeyboardProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

function imageFile(name: string): File {
  return new File(['png'], name, { type: 'image/png' });
}

describe('ticket description attachments', () => {
  beforeEach(() => {
    upload.mockReset();
    updateTicket.mockReset();
    upload.mockResolvedValue({
      id: '00000000-0000-4000-8000-000000000099',
    });
    updateTicket.mockResolvedValue(ticket);
    vi.stubGlobal('URL', {
      ...URL,
      createObjectURL: () => 'blob:shot',
      revokeObjectURL: () => {},
    });
  });

  it('shows Attach files on the description and the comment box', () => {
    renderPanel();
    const buttons = screen.getAllByRole('button', { name: 'Attach files' });
    expect(buttons).toHaveLength(2);
    for (const button of buttons) {
      expect(button).toHaveAttribute('title', 'Attach files');
    }
  });

  it('uploads a file dropped on the ticket and links it to the description', async () => {
    renderPanel();
    const shot = imageFile('shot.png');
    fireEvent.drop(screen.getByText('Ticket description'), {
      dataTransfer: { types: ['Files'], files: [shot] },
    });
    await waitFor(() => expect(upload).toHaveBeenCalledWith(shot));
    await waitFor(() =>
      expect(updateTicket).toHaveBeenCalledWith({
        attachmentIds: ['00000000-0000-4000-8000-000000000099'],
      }),
    );
  });

  it('uploads an image pasted on the description', async () => {
    renderPanel();
    const shot = imageFile('pasted.png');
    fireEvent.paste(screen.getByText('Ticket description'), {
      clipboardData: {
        items: [{ kind: 'file', type: 'image/png', getAsFile: () => shot }],
      },
    });
    await waitFor(() => expect(upload).toHaveBeenCalledWith(shot));
  });

  it('keeps a comment drop as a pending chip and does not attach it to the ticket', () => {
    renderPanel();
    const shot = imageFile('note.png');
    const form = screen.getByLabelText('Comment').closest('form');
    fireEvent.dragOver(form!, { dataTransfer: { types: ['Files'], files: [shot] } });
    expect(screen.getByText('Drop to add to your comment')).toBeInTheDocument();
    expect(screen.queryByText('Drop to attach')).not.toBeInTheDocument();
    fireEvent.drop(form!, { dataTransfer: { types: ['Files'], files: [shot] } });
    expect(screen.getByAltText('note.png')).toBeInTheDocument();
    expect(upload).not.toHaveBeenCalled();
  });

  it('adds a pending chip when an image is pasted into the comment box', () => {
    renderPanel();
    const shot = imageFile('comment.png');
    const form = screen.getByLabelText('Comment').closest('form');
    fireEvent.paste(form!, {
      clipboardData: {
        items: [{ kind: 'file', type: 'image/png', getAsFile: () => shot }],
      },
    });
    expect(screen.getByAltText('comment.png')).toBeInTheDocument();
    expect(upload).not.toHaveBeenCalled();
  });

  it('toasts the locked failure sentences and does not upload', () => {
    renderPanel();
    const html = new File(['<p>'], 'page.html', { type: 'text/html' });
    const huge = new File([new Uint8Array(10 * 1024 * 1024 + 1)], 'diagram.png', {
      type: 'image/png',
    });
    fireEvent.drop(screen.getByText('Ticket description'), {
      dataTransfer: { types: ['Files'], files: [html, huge] },
    });
    expect(screen.getByText("Coppice can't attach html files.")).toBeInTheDocument();
    expect(
      screen.getByText('diagram.png is too large. The limit is 10.0 MB.'),
    ).toBeInTheDocument();
    expect(upload).not.toHaveBeenCalled();
  });
});
