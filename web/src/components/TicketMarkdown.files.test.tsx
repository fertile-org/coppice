import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { FilePreviewProvider } from '../features/files/FilePreviewProvider';
import { TicketMarkdown } from './TicketMarkdown';

const MARKDOWN = [
  'Updated server/src/api/mod.rs and `web/src/App.tsx:2`.',
  '',
  'See [the screenshot](docs/shot.png) and [docs](https://example.com/a.md).',
].join('\n');

function renderWithPreview(ticketId: string | null) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <FilePreviewProvider ticketId={ticketId}>
        <TicketMarkdown>{MARKDOWN}</TicketMarkdown>
      </FilePreviewProvider>
    </QueryClientProvider>,
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('TicketMarkdown file references', () => {
  it('renders mentioned files as preview buttons and keeps external links', () => {
    renderWithPreview('ticket-1');
    expect(screen.getByRole('button', { name: 'server/src/api/mod.rs' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'web/src/App.tsx:2' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'the screenshot' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'docs' })).toHaveAttribute(
      'href',
      'https://example.com/a.md',
    );
  });

  it('leaves paths as text without a ticket repository', () => {
    renderWithPreview(null);
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
    expect(screen.getByText(/server\/src\/api\/mod\.rs/)).toBeInTheDocument();
  });

  it('previews a text file and highlights the mentioned line', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response('line one\nline two\n', {
        status: 200,
        headers: { 'x-coppice-file-source': 'worktree' },
      }),
    );
    vi.stubGlobal('fetch', fetchMock);
    renderWithPreview('ticket-1');

    fireEvent.click(screen.getByRole('button', { name: 'web/src/App.tsx:2' }));

    const dialog = await screen.findByRole('dialog');
    expect(within(dialog).getByText('web/src/App.tsx:2')).toBeInTheDocument();
    expect(await within(dialog).findByText(/line two/)).toBeInTheDocument();
    expect(within(dialog).getByText(/Ticket worktree/)).toBeInTheDocument();
    expect(within(dialog).getByTestId('file-preview-highlight')).toBeInTheDocument();
    expect(fetchMock.mock.calls[0][0]).toBe(
      '/api/tickets/ticket-1/files/content?path=web%2Fsrc%2FApp.tsx',
    );
  });

  it('explains when a file is missing', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('', { status: 404 })));
    renderWithPreview('ticket-1');

    fireEvent.click(screen.getByRole('button', { name: 'server/src/api/mod.rs' }));

    expect(
      await screen.findByText(/not found in the ticket worktree or the repository checkout/),
    ).toBeInTheDocument();
  });

  it('previews images inline', async () => {
    renderWithPreview('ticket-1');
    fireEvent.click(screen.getByRole('button', { name: 'the screenshot' }));
    const image = await screen.findByRole('img', { name: 'docs/shot.png' });
    expect(image).toHaveAttribute('src', '/api/tickets/ticket-1/files/content?path=docs%2Fshot.png');
  });
});
