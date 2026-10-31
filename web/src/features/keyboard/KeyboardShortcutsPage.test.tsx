import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SessionContext, type SessionContextValue } from '../auth/useSession';
import { HELP_COPY } from './help';
import { KeyboardProvider } from './KeyboardProvider';
import { KeyboardShortcutsPage } from './KeyboardShortcutsPage';
import { publishKeybindings } from './keybindings';
import { PALETTE_COPY, PALETTE_HELP_LABEL } from './palette';

const memberSession: SessionContextValue = {
  user: { id: 'user-1', email: 'ada@example.com', role: 'member' },
  loading: false,
  desktopMode: false,
  establishSession: () => undefined,
  logout: async () => undefined,
};

const adminSession: SessionContextValue = {
  ...memberSession,
  user: { id: 'user-1', email: 'ada@example.com', role: 'admin' },
};

function renderPage(session: SessionContextValue = memberSession) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={['/help/shortcuts']}>
        <SessionContext.Provider value={session}>
          <KeyboardProvider>
            <KeyboardShortcutsPage />
          </KeyboardProvider>
        </SessionContext.Provider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('keyboard shortcuts page', () => {
  beforeEach(() => {
    publishKeybindings({});
    vi.stubGlobal('fetch', vi.fn(() => Promise.reject(new Error('offline'))));
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('lists Navigate, Tickets, and Review with the footer', () => {
    renderPage();

    expect(screen.getByRole('heading', { name: HELP_COPY.title })).toBeVisible();
    expect(screen.getByText(HELP_COPY.pageIntro)).toBeVisible();
    expect(screen.getByRole('heading', { name: 'Navigate' })).toBeVisible();
    expect(screen.getByRole('heading', { name: 'Tickets' })).toBeVisible();
    expect(screen.getByRole('heading', { name: 'Review' })).toBeVisible();
    expect(screen.getByText('Go to column 1')).toBeVisible();
    expect(screen.getByText('G then 1')).toBeVisible();
    expect(screen.getByText('Show keyboard shortcuts')).toBeVisible();
    expect(screen.getByText('Create ticket')).toBeVisible();
    expect(screen.getByText(HELP_COPY.footer)).toBeVisible();
    expect(screen.queryByText('Go to Plugins')).toBeNull();
  });

  it('shows the same keys on the page, overlay, and palette, including custom bindings', () => {
    publishKeybindings({
      'help.show': [{ kind: 'chord', chord: { key: '/' } }],
      'nav.boards': [{ kind: 'sequence', keys: ['g', 'x'] }],
    });
    renderPage();

    expect(screen.getByText('Go to Boards').closest('li')).toHaveTextContent('G then X');
    expect(screen.getByText('Show keyboard shortcuts').closest('li')).toHaveTextContent('/');

    fireEvent.keyDown(document, { key: '/' });
    const overlay = screen.getByRole('dialog', { name: 'Keyboard shortcuts' });
    expect(within(overlay).getByText('Go to Boards').closest('li')).toHaveTextContent('G then X');
    expect(within(overlay).getByText('Show keyboard shortcuts').closest('li')).toHaveTextContent(
      '/',
    );

    fireEvent.keyDown(document, { key: 'Escape' });
    fireEvent.keyDown(document, { key: 'k', ctrlKey: true });
    expect(screen.getByPlaceholderText(PALETTE_COPY.placeholder)).toBeVisible();
    expect(screen.getByRole('option', { name: /Go to Boards/ })).toHaveTextContent('G then X');
    expect(screen.getByRole('option', { name: new RegExp(PALETTE_HELP_LABEL) })).toHaveTextContent(
      '/',
    );
  });

  it('omits Users on the desktop app and keeps Settings', () => {
    renderPage({ ...adminSession, desktopMode: true });

    expect(screen.getByText('Go to Settings')).toBeVisible();
    expect(screen.getByText('Go to Plugins')).toBeVisible();
    expect(screen.queryByText('Go to Users')).toBeNull();
  });

  it('adds the skipped-shortcut footer when a binding was skipped', () => {
    publishKeybindings(
      {
        'palette.open': [{ kind: 'chord', chord: { key: 'p', mod: true } }],
      },
      ['No command called "nope". No shortcut was changed.'],
    );
    renderPage();

    expect(screen.getByText(HELP_COPY.footer)).toBeVisible();
    expect(screen.getByText(HELP_COPY.skipped)).toBeVisible();
    expect(screen.getByText('Ctrl+P')).toBeVisible();
  });
});
