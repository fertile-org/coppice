import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { TicketMarkdown } from '../../components/TicketMarkdown';
import { FilePreviewProvider } from './FilePreviewProvider';
import { MAX_HIGHLIGHT_BYTES } from './FilePreviewModal';
import type { FileReference } from './fileReference';
import { FilePreviewModal } from './FilePreviewModal';

function renderModal(file: FileReference, ticketId = 'ticket-1') {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <FilePreviewModal open ticketId={ticketId} file={file} onClose={() => undefined} />
    </QueryClientProvider>,
  );
}

function renderMarkdownPreview(markdown: string, basePath: string) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <FilePreviewProvider ticketId="ticket-1">
        <TicketMarkdown basePath={basePath} ticketId="ticket-1">
          {markdown}
        </TicketMarkdown>
      </FilePreviewProvider>
    </QueryClientProvider>,
  );
}

function mockTextFile(text: string, path = 'docs/a.md') {
  vi.stubGlobal(
    'fetch',
    vi.fn().mockResolvedValue(
      new Response(text, {
        status: 200,
        headers: {
          'x-coppice-file-source': 'worktree',
          'x-coppice-file-path': encodeURIComponent(path),
        },
      }),
    ),
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('FilePreviewModal markdown preview', () => {
  it('opens markdown in Preview with a Preview | Source toggle', async () => {
    mockTextFile('# Hello\n\nBody text.\n');
    renderModal({ path: 'docs/a.md', kind: 'text' });

    const dialog = await screen.findByRole('dialog');
    expect(await within(dialog).findByTestId('file-preview-markdown')).toBeInTheDocument();
    expect(within(dialog).getByRole('heading', { name: 'Hello' })).toBeInTheDocument();

    const tablist = within(dialog).getByRole('tablist', { name: 'View mode' });
    expect(within(tablist).getByRole('tab', { name: 'Preview' })).toHaveAttribute(
      'aria-selected',
      'true',
    );

    fireEvent.click(within(tablist).getByRole('tab', { name: 'Source' }));
    expect(await within(dialog).findByTestId('file-preview-text')).toBeInTheDocument();
    expect(within(dialog).queryByTestId('file-preview-markdown')).not.toBeInTheDocument();
    expect(within(dialog).getByText(/# Hello/)).toBeInTheDocument();

    fireEvent.click(within(tablist).getByRole('tab', { name: 'Preview' }));
    expect(await within(dialog).findByTestId('file-preview-markdown')).toBeInTheDocument();
  });

  it('opens a line reference in Source with the line highlighted', async () => {
    mockTextFile(Array.from({ length: 15 }, (_, i) => `line ${i + 1}`).join('\n'), 'README.md');
    renderModal({ path: 'README.md', kind: 'text', line: 12 });

    const dialog = await screen.findByRole('dialog');
    expect(await within(dialog).findByTestId('file-preview-text')).toBeInTheDocument();
    expect(within(dialog).getByTestId('file-preview-highlight')).toBeInTheDocument();
    expect(within(dialog).getByRole('tab', { name: 'Source' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    expect(within(dialog).queryByTestId('file-preview-markdown')).not.toBeInTheDocument();
  });

  it('hides the toggle for code files', async () => {
    mockTextFile('fn main() {}\n', 'server/src/main.rs');
    renderModal({ path: 'server/src/main.rs', kind: 'text' });

    const dialog = await screen.findByRole('dialog');
    expect(await within(dialog).findByTestId('file-preview-text')).toBeInTheDocument();
    expect(within(dialog).queryByRole('tablist', { name: 'View mode' })).not.toBeInTheDocument();
  });

  it('hides the toggle for images', async () => {
    renderModal({ path: 'docs/shot.png', kind: 'image' });
    expect(await screen.findByRole('img', { name: 'docs/shot.png' })).toBeInTheDocument();
    expect(screen.queryByRole('tablist', { name: 'View mode' })).not.toBeInTheDocument();
  });

  it('forces Source only past MAX_HIGHLIGHT_BYTES', async () => {
    const huge = `# Big\n\n${'x'.repeat(MAX_HIGHLIGHT_BYTES)}\n`;
    mockTextFile(huge);
    renderModal({ path: 'docs/a.md', kind: 'text' });

    const dialog = await screen.findByRole('dialog');
    expect(await within(dialog).findByTestId('file-preview-text')).toBeInTheDocument();
    expect(within(dialog).queryByRole('tablist', { name: 'View mode' })).not.toBeInTheDocument();
    expect(within(dialog).queryByTestId('file-preview-markdown')).not.toBeInTheDocument();
  });
});

describe('TicketMarkdown relative file preview links', () => {
  it('resolves relative images and markdown links against the previewed file', async () => {
    renderMarkdownPreview(
      'See [ops](ops.md) and ![](shot.png)',
      'docs/a.md',
    );

    expect(screen.getByRole('button', { name: 'ops' })).toBeInTheDocument();
    const image = screen.getByRole('img', { name: 'docs/shot.png' });
    expect(image).toHaveAttribute(
      'src',
      '/api/tickets/ticket-1/files/content?path=docs%2Fshot.png',
    );

    mockTextFile('# Ops\n', 'docs/ops.md');
    fireEvent.click(screen.getByRole('button', { name: 'ops' }));
    const dialog = await screen.findByRole('dialog');
    expect(await within(dialog).findByRole('heading', { name: 'Ops' })).toBeInTheDocument();
  });

  it('does not turn repo-escaping paths into file links or image URLs', () => {
    renderMarkdownPreview(
      'Bad [secret](../../secret.md) and ![](../../shot.png)',
      'docs/a.md',
    );

    expect(screen.queryByRole('button', { name: 'secret' })).not.toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'secret' })).toHaveAttribute(
      'href',
      '../../secret.md',
    );
    expect(screen.queryByRole('img')).not.toBeInTheDocument();
  });
});
