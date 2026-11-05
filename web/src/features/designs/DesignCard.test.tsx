import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DesignCard } from './DesignCard';
import { makeDesignSet, makeOption, makePick } from './designFixtures';
import { rewriteDesignHtml } from './rewriteDesignHtml';

const apiFetch = vi.hoisted(() => vi.fn());

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch,
}));

const published = makeDesignSet({
  options: [
    makeOption('a', 'Quiet', 'A'),
    makeOption('b', 'Loud', 'B'),
  ],
  recommended: null,
  selected: null,
  choice: { mode: 'save_only', planOutOfDate: false },
});

const PREVIEW_CSP =
  "default-src 'none'; script-src 'unsafe-inline' 'unsafe-eval' blob:; style-src 'unsafe-inline' data:; img-src data:; font-src data:; connect-src 'none'; frame-src 'none'";
const CSP_META = `<meta http-equiv="Content-Security-Policy" content="${PREVIEW_CSP}">`;
const PNG_DATA = 'data:image/png;base64,AQ==';

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
    expect(screen.getByText('A: Quiet')).toBeInTheDocument();
    expect(screen.getByText('B: Loud')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Open' }));

    const dialog = await screen.findByRole('dialog');
    expect(within(dialog).getByRole('tab', { name: 'A: Quiet' })).toBeInTheDocument();
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
    wrap(<DesignCard design={{ ...published, chosen: makePick(makeOption('a', 'Quiet', 'A')) }} />);
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
    const frame = await screen.findByTitle('A: Quiet');
    expect(frame.tagName).toBe('IFRAME');
    expect(frame).toHaveAttribute('sandbox', 'allow-scripts');
    expect(frame.getAttribute('src')).toMatch(/^blob:/);
  });

  it('inlines a linked stylesheet and rewrites images to data urls', async () => {
    apiFetch.mockImplementation((path: string) => {
      const file = String(path);
      if (file.endsWith('/a/index.html')) {
        return Promise.resolve(
          new Response(
            '<link rel="stylesheet" href="https://cdn.example/x.css"><link rel="stylesheet" href="app.css"><img src="shot.png"><img src="icon.svg"><img src="https://cdn.example/x.png"><img src="data:image/png;base64,qq">',
            { status: 200 },
          ),
        );
      }
      if (file.endsWith('/a/app.css')) {
        return Promise.resolve(
          new Response(
            '@import url("extra.css");body{background:url("bg.png");src:url("f.woff");src:url("f.woff2")}',
            { status: 200 },
          ),
        );
      }
      if (file.endsWith('/a/extra.css')) {
        return Promise.resolve(new Response('h1{color:blue}', { status: 200 }));
      }
      if (file.endsWith('/a/shot.png') || file.endsWith('/a/bg.png')) {
        return Promise.resolve(new Response(new Uint8Array([1]), { status: 200 }));
      }
      if (file.endsWith('/a/icon.svg')) {
        return Promise.resolve(new Response(new Uint8Array([2]), { status: 200 }));
      }
      if (file.endsWith('/a/f.woff')) {
        return Promise.resolve(new Response(new Uint8Array([3]), { status: 200 }));
      }
      if (file.endsWith('/a/f.woff2')) {
        return Promise.resolve(new Response(new Uint8Array([4]), { status: 200 }));
      }
      return Promise.resolve(new Response('missing', { status: 404 }));
    });

    const page = await rewriteDesignHtml(published.id, 'a/index.html');
    const html = blobs.at(-1);
    expect(html?.type).toBe('text/html');
    expect(blobs).toHaveLength(1);
    expect(await readBlob(html)).toBe(
      `${CSP_META}<link rel="stylesheet" href="https://cdn.example/x.css"><style>@import url("data:text/css;base64,${btoa('h1{color:blue}')}");body{background:url("${PNG_DATA}");src:url("data:font/woff;base64,Aw==");src:url("data:font/woff2;base64,BA==")}</style><img src="${PNG_DATA}"><img src="data:image/svg+xml;base64,Ag=="><img src="https://cdn.example/x.png"><img src="data:image/png;base64,qq">`,
    );
    expect(page.url).toBe('blob:asset-1');
    page.revoke();
  });

  it('inlines one shared stylesheet into each option that links it', async () => {
    apiFetch.mockImplementation((path: string) => {
      const file = String(path);
      if (file.endsWith('/a.html')) {
        return Promise.resolve(
          new Response('<link rel="stylesheet" href="shared.css"><p>A</p>', { status: 200 }),
        );
      }
      if (file.endsWith('/b.html')) {
        return Promise.resolve(
          new Response(`<link href='shared.css' rel='stylesheet'><p>B</p>`, { status: 200 }),
        );
      }
      if (file.endsWith('/shared.css')) {
        return Promise.resolve(new Response('body{color:red}', { status: 200 }));
      }
      return Promise.resolve(new Response('missing', { status: 404 }));
    });

    await rewriteDesignHtml(published.id, 'a.html');
    await rewriteDesignHtml(published.id, 'b.html');
    expect(await readBlob(blobs[0])).toContain('<style>body{color:red}</style>');
    expect(await readBlob(blobs[1])).toContain('<style>body{color:red}</style>');
    expect(blobs.every((blob) => blob.type === 'text/html')).toBe(true);
  });

  it('leaves a missing stylesheet link in place', async () => {
    apiFetch.mockImplementation((path: string) => {
      if (String(path).endsWith('/a/index.html')) {
        return Promise.resolve(
          new Response('<link rel="stylesheet" href="nope.css"><p>Hi</p>', { status: 200 }),
        );
      }
      return Promise.resolve(new Response('missing', { status: 404 }));
    });

    await rewriteDesignHtml(published.id, 'a/index.html');
    expect(await readBlob(blobs[0])).toContain('<link rel="stylesheet" href="nope.css">');
  });

  it('places the preview CSP after head and keeps the doctype first', async () => {
    apiFetch.mockImplementation((path: string) => {
      if (String(path).endsWith('/a/index.html')) {
        return Promise.resolve(
          new Response('<!doctype html><html><head><title>T</title></head><body></body></html>', {
            status: 200,
          }),
        );
      }
      return Promise.resolve(new Response('missing', { status: 404 }));
    });

    await rewriteDesignHtml(published.id, 'a/index.html');
    const html = await readBlob(blobs[0]);
    expect(html.startsWith('<!doctype html>')).toBe(true);
    expect(html.indexOf('<head>')).toBeLessThan(html.indexOf(CSP_META));
    expect(html.indexOf(CSP_META)).toBeLessThan(html.indexOf('<title>'));
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
    const html = await readBlob(blobs[0]);
    const href = /href="([^"]+)"/.exec(html)?.[1] ?? '';
    const payload = new TextDecoder().decode(
      Uint8Array.from(atob(href.split(',')[1] ?? ''), (char) => char.charCodeAt(0)),
    );
    expect(href.startsWith('data:text/html;base64,')).toBe(true);
    expect(payload).toBe(`${CSP_META}<p>Nested</p>`);
    expect(blobs).toHaveLength(1);
  });

  it('revokes nothing when a later file aborts before the page blob exists', async () => {
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
    expect(revoked).toEqual([]);
    expect(blobs).toHaveLength(0);
  });
});
