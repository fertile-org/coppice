import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DesignCard } from './DesignCard';
import { makeDesignSet } from './designFixtures';
import { rewriteDesignHtml } from './rewriteDesignHtml';

const apiFetch = vi.hoisted(() => vi.fn());

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch,
}));

const published = makeDesignSet({
  options: [
    { id: 'a', title: 'A', entry: 'a/index.html' },
    { id: 'b', title: 'B', entry: 'b/index.html' },
  ],
  recommended: null,
  selected: null,
  choice: { mode: 'save_only', planOutOfDate: false },
});

const blobs: Blob[] = [];

function readBlob(blob: Blob | undefined): Promise<string> {
  if (!blob) return Promise.reject(new Error('missing blob'));
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error);
    reader.readAsText(blob);
  });
}

function wrap(node: ReactNode) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(<QueryClientProvider client={client}>{node}</QueryClientProvider>);
}

describe('DesignCard', () => {
  beforeEach(() => {
    blobs.length = 0;
    Object.defineProperty(URL, 'createObjectURL', {
      configurable: true,
      writable: true,
      value: (blob: Blob) => {
        blobs.push(blob);
        return `blob:asset-${blobs.length}`;
      },
    });
    Object.defineProperty(URL, 'revokeObjectURL', {
      configurable: true,
      writable: true,
      value: () => {},
    });
    apiFetch.mockReset();
    apiFetch.mockImplementation((path: string) => {
      if (String(path).endsWith('/choose')) {
        return Promise.resolve(new Response(null, { status: 204 }));
      }
      return Promise.resolve(new Response('missing', { status: 404 }));
    });
  });

  it('opens the popup and posts Choose', async () => {
    wrap(<DesignCard design={published} />);

    expect(screen.getByText('Landing page')).toBeInTheDocument();
    expect(screen.getByText('A')).toBeInTheDocument();
    expect(screen.getByText('B')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Open' }));

    const dialog = await screen.findByRole('dialog');
    expect(within(dialog).getByRole('tab', { name: 'A' })).toBeInTheDocument();
    expect(await within(dialog).findByText('This preview has no file.')).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole('button', { name: 'Choose A' }));

    await waitFor(() => {
      expect(apiFetch).toHaveBeenCalledWith(
        '/api/designs/11111111-1111-4111-8111-111111111111/choose',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ optionId: 'a' }),
        }),
      );
    });
  });

  it('shows the Chosen marker once the list says so', () => {
    wrap(<DesignCard design={{ ...published, chosen: { id: 'a', title: 'A' } }} />);
    expect(screen.getByText('Chosen')).toBeInTheDocument();
  });

  it('previews the entry in a sandboxed iframe', async () => {
    apiFetch.mockImplementation((path: string) => {
      if (String(path).endsWith('/a/index.html')) {
        return Promise.resolve(new Response('<p>Hi</p>', { status: 200 }));
      }
      return Promise.resolve(new Response('missing', { status: 404 }));
    });
    wrap(<DesignCard design={published} />);
    fireEvent.click(screen.getByRole('button', { name: 'Open' }));
    const frame = await screen.findByTitle('A');
    expect(frame.tagName).toBe('IFRAME');
    expect(frame).toHaveAttribute('sandbox', 'allow-scripts');
    expect(frame.getAttribute('src')).toMatch(/^blob:/);
  });

  it('rewrites relative css and images onto blob urls', async () => {
    apiFetch.mockImplementation((path: string) => {
      const file = String(path);
      if (file.endsWith('/a/index.html')) {
        return Promise.resolve(
          new Response(
            '<link rel="stylesheet" href="app.css"><img src="shot.png"><img src="https://cdn.example/x.png"><img src="data:image/png;base64,qq">',
            { status: 200 },
          ),
        );
      }
      if (file.endsWith('/a/app.css')) {
        return Promise.resolve(new Response('body{background:url("bg.png")}', { status: 200 }));
      }
      if (file.endsWith('/a/shot.png') || file.endsWith('/a/bg.png')) {
        return Promise.resolve(new Response(new Uint8Array([1]), { status: 200 }));
      }
      return Promise.resolve(new Response('missing', { status: 404 }));
    });

    const page = await rewriteDesignHtml(published.id, 'a/index.html');
    const html = blobs.at(-1);
    expect(html?.type).toBe('text/html');
    expect(await readBlob(html)).toBe(
      `<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline' 'unsafe-eval' blob:; style-src 'unsafe-inline' blob: data:; img-src blob: data:; font-src blob: data:; connect-src 'none'; frame-src 'none'"><link rel="stylesheet" href="blob:asset-2"><img src="blob:asset-3"><img src="https://cdn.example/x.png"><img src="data:image/png;base64,qq">`,
    );
    const css = blobs.find((blob) => blob.type === 'text/css');
    expect(await readBlob(css)).toBe('body{background:url("blob:asset-1")}');
    expect(page.url).toBe('blob:asset-4');
    page.revoke();
  });

  it('adds the preview CSP to a nested html file', async () => {
    apiFetch.mockImplementation((path: string) => {
      const file = String(path);
      if (file.endsWith('/a/index.html')) {
        return Promise.resolve(new Response('<a href="page.HTML">Next</a>', { status: 200 }));
      }
      if (file.endsWith('/a/page.HTML')) {
        return Promise.resolve(new Response('<p>Nested</p>', { status: 200 }));
      }
      return Promise.resolve(new Response('missing', { status: 404 }));
    });

    await rewriteDesignHtml(published.id, 'a/index.html');
    expect(await readBlob(blobs[0])).toBe(
      `<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline' 'unsafe-eval' blob:; style-src 'unsafe-inline' blob: data:; img-src blob: data:; font-src blob: data:; connect-src 'none'; frame-src 'none'"><p>Nested</p>`,
    );
  });

  it('revokes blob urls when a later file aborts', async () => {
    const revoked: string[] = [];
    Object.defineProperty(URL, 'revokeObjectURL', {
      configurable: true,
      writable: true,
      value: (url: string) => {
        revoked.push(url);
      },
    });
    apiFetch.mockImplementation((path: string) => {
      const file = String(path);
      if (file.endsWith('/a/index.html')) {
        return Promise.resolve(new Response('<img src="ok.png"><img src="later.png">', { status: 200 }));
      }
      if (file.endsWith('/a/ok.png')) {
        return Promise.resolve(new Response(new Uint8Array([1]), { status: 200 }));
      }
      const abort = new Error('aborted');
      abort.name = 'AbortError';
      return Promise.reject(abort);
    });

    await expect(rewriteDesignHtml(published.id, 'a/index.html')).rejects.toThrow(/aborted/);
    expect(revoked).toEqual(['blob:asset-1']);
  });
});
