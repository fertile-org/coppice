import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { FilePreviewProvider } from '../features/files/FilePreviewProvider';
import { TicketMarkdown } from './TicketMarkdown';

describe('TicketMarkdown document variant', () => {
  it('renders SKILL.md frontmatter as a Properties block without a rule or bold heading', () => {
    const skill = [
      '---',
      'name: coppice-qc-verification',
      'description: QC rules for verifying in_qa tickets - verification only.',
      '---',
      '',
      '# Coppice platform rules — QA verification',
      '',
      'These rules override conflicting instructions.',
    ].join('\n');

    render(
      <TicketMarkdown variant="document" basePath="skills/SKILL.md" ticketId="ticket-1">
        {skill}
      </TicketMarkdown>,
    );

    const props = screen.getByTestId('markdown-frontmatter');
    expect(within(props).getByText('Properties')).toBeInTheDocument();
    expect(within(props).getByText('name')).toBeInTheDocument();
    expect(within(props).getByText('coppice-qc-verification')).toBeInTheDocument();
    expect(within(props).getByText('description')).toBeInTheDocument();
    expect(
      within(props).getByText(
        'QC rules for verifying in_qa tickets - verification only.',
      ),
    ).toBeInTheDocument();
    expect(props.querySelector('hr')).toBeNull();
    expect(within(props).queryByRole('heading')).not.toBeInTheDocument();
    expect(
      screen.getByRole('heading', { level: 1, name: /Coppice platform rules/ }),
    ).toBeInTheDocument();
    expect(screen.queryByText(/^name:/)).not.toBeInTheDocument();
  });

  it('renders the full document when frontmatter is missing or unclosed', () => {
    const { rerender } = render(
      <TicketMarkdown variant="document">{'# Only body'}</TicketMarkdown>,
    );
    expect(screen.queryByTestId('markdown-frontmatter')).not.toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Only body' })).toBeInTheDocument();

    rerender(
      <TicketMarkdown variant="document">
        {'---\nname: orphan\n\n# Still rendered'}
      </TicketMarkdown>,
    );
    expect(screen.queryByTestId('markdown-frontmatter')).not.toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Still rendered' })).toBeInTheDocument();
    expect(screen.getByText(/name: orphan/)).toBeInTheDocument();
  });

  it('keeps the default comment variant compact (no Properties, 14px body classes)', () => {
    render(
      <TicketMarkdown>
        {[
          '---',
          'name: should-not-parse',
          '---',
          '',
          'Comment body.',
        ].join('\n')}
      </TicketMarkdown>,
    );
    expect(screen.queryByTestId('markdown-frontmatter')).not.toBeInTheDocument();
    expect(screen.getByText('Comment body.')).toHaveClass('text-sm');
  });
});


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
    expect(screen.getByRole('link', { name: 'docs' })).toHaveAttribute(
      'target',
      '_blank',
    );
    expect(screen.getByRole('link', { name: 'docs' })).toHaveAttribute(
      'rel',
      'noreferrer',
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
        headers: {
          'x-coppice-file-source': 'worktree',
          'x-coppice-file-path': 'web%2Fsrc%2FApp.tsx',
        },
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
    expect(within(dialog).getByRole('link', { name: /open raw/i })).toBeInTheDocument();
    expect(fetchMock.mock.calls[0][0]).toBe(
      '/api/tickets/ticket-1/files/content?path=web%2Fsrc%2FApp.tsx',
    );
  });

  it('shows the resolved path and keeps the line suffix', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response('export {}\n', {
          status: 200,
          headers: {
            'x-coppice-file-source': 'repo',
            'x-coppice-file-path': 'web%2Fsrc%2Fcomponents%2FAppShell.test.tsx',
          },
        }),
      ),
    );
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <FilePreviewProvider ticketId="ticket-1">
          <TicketMarkdown>{'See `AppShell.test.tsx:4`.'}</TicketMarkdown>
        </FilePreviewProvider>
      </QueryClientProvider>,
    );

    fireEvent.click(screen.getByRole('button', { name: 'AppShell.test.tsx:4' }));

    const dialog = await screen.findByRole('dialog');
    expect(await within(dialog).findByText('web/src/components/AppShell.test.tsx:4')).toBeInTheDocument();
  });

  it('explains when a file is missing and hides Open raw', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('', { status: 404 })));
    renderWithPreview('ticket-1');

    fireEvent.click(screen.getByRole('button', { name: 'server/src/api/mod.rs' }));

    expect(
      await screen.findByText(/not found in the ticket worktree or the repository checkout/),
    ).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: /open raw/i })).not.toBeInTheDocument();
  });

  it('asks for the full path when several files match', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('', { status: 409 })));
    renderWithPreview('ticket-1');

    fireEvent.click(screen.getByRole('button', { name: 'server/src/api/mod.rs' }));

    expect(
      await screen.findByText(/matches several files, use the full path/i),
    ).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: /open raw/i })).not.toBeInTheDocument();
  });

  it('previews images inline', async () => {
    renderWithPreview('ticket-1');
    fireEvent.click(screen.getByRole('button', { name: 'the screenshot' }));
    const image = await screen.findByRole('img', { name: 'docs/shot.png' });
    expect(image).toHaveAttribute('src', '/api/tickets/ticket-1/files/content?path=docs%2Fshot.png');
    expect(screen.getByRole('link', { name: /open raw/i })).toBeInTheDocument();
  });

  it('hides Open raw when an image fails to load', async () => {
    renderWithPreview('ticket-1');
    fireEvent.click(screen.getByRole('button', { name: 'the screenshot' }));
    const image = await screen.findByRole('img', { name: 'docs/shot.png' });
    fireEvent.error(image);
    expect(await screen.findByText(/unable to load this image/i)).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: /open raw/i })).not.toBeInTheDocument();
  });
});
