import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, useLocation } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SessionContext, type SessionContextValue } from '../auth/useSession';
import { KeyboardProvider } from './KeyboardProvider';
import { publishKeybindings } from './keybindings';

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

function LocationProbe() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}</div>;
}

function renderHelp(session: SessionContextValue | null = memberSession) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={['/boards/board-1']}>
        <SessionContext.Provider value={session}>
          <KeyboardProvider>
            <button type="button">Before</button>
            <LocationProbe />
          </KeyboardProvider>
        </SessionContext.Provider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('shortcut help overlay', () => {
  beforeEach(() => {
    publishKeybindings({});
    vi.stubGlobal('fetch', vi.fn(() => Promise.reject(new Error('offline'))));
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('opens with ?, lists Go to column {n}, and restores focus on Esc', async () => {
    renderHelp();
    const before = screen.getByRole('button', { name: 'Before' });
    before.focus();

    fireEvent.keyDown(document, { key: '?' });
    const dialog = screen.getByRole('dialog', { name: 'Keyboard shortcuts' });
    expect(screen.getByRole('heading', { name: 'Navigate' })).toBeVisible();
    expect(screen.getByRole('heading', { name: 'Tickets' })).toBeVisible();
    expect(screen.getByRole('heading', { name: 'Review' })).toBeVisible();
    expect(screen.getByText('Go to column 1')).toBeVisible();
    expect(screen.getByText('Go to column 9')).toBeVisible();
    expect(screen.getByText('G then 1')).toBeVisible();
    expect(screen.queryByText('Go to Backlog column')).toBeNull();
    expect(screen.getByText('Show keyboard shortcuts')).toBeVisible();
    expect(screen.getByText('Create ticket')).toBeVisible();
    expect(screen.getByText('Accept…')).toBeVisible();
    expect(screen.getByText('Ctrl+Shift+Enter')).toBeVisible();
    expect(screen.getByText('Change shortcuts in Settings.')).toBeVisible();
    expect(screen.queryByText('Go to Plugins')).toBeNull();
    expect(screen.queryByText('Go to Users')).toBeNull();

    fireEvent.keyDown(dialog, { key: 'Escape' });
    await waitFor(() => {
      expect(screen.queryByRole('dialog', { name: 'Keyboard shortcuts' })).toBeNull();
    });
    expect(before).toHaveFocus();
  });

  it('toggles closed with ? and does not open from a field or the terminal', () => {
    renderHelp();

    fireEvent.keyDown(document, { key: '?' });
    expect(screen.getByRole('dialog', { name: 'Keyboard shortcuts' })).toBeVisible();
    fireEvent.keyDown(document, { key: '?' });
    expect(screen.queryByRole('dialog', { name: 'Keyboard shortcuts' })).toBeNull();

    const input = document.createElement('input');
    document.body.appendChild(input);
    input.focus();
    const fromField = new KeyboardEvent('keydown', {
      key: '?',
      bubbles: true,
      cancelable: true,
    });
    input.dispatchEvent(fromField);
    expect(fromField.defaultPrevented).toBe(false);
    expect(screen.queryByRole('dialog', { name: 'Keyboard shortcuts' })).toBeNull();
    input.remove();

    const terminal = document.createElement('div');
    terminal.className = 'xterm';
    const termInput = document.createElement('textarea');
    terminal.appendChild(termInput);
    document.body.appendChild(terminal);
    termInput.focus();
    const fromTerminal = new KeyboardEvent('keydown', {
      key: '?',
      bubbles: true,
      cancelable: true,
    });
    termInput.dispatchEvent(fromTerminal);
    expect(fromTerminal.defaultPrevented).toBe(false);
    expect(screen.queryByRole('dialog', { name: 'Keyboard shortcuts' })).toBeNull();
    terminal.remove();
  });

  it('opens from the command palette and jumps to pages with G then a letter', async () => {
    renderHelp(adminSession);

    fireEvent.keyDown(document, { key: 'k', ctrlKey: true });
    const palette = screen.getByPlaceholderText('Search actions, tickets, and agents…');
    fireEvent.change(palette, { target: { value: 'keyboard shortcuts' } });
    fireEvent.click(screen.getByRole('option', { name: /Show keyboard shortcuts/ }));
    expect(await screen.findByRole('dialog', { name: 'Keyboard shortcuts' })).toBeVisible();
    expect(screen.getByText('Go to Plugins')).toBeVisible();
    expect(screen.getByText('Go to Users')).toBeVisible();

    fireEvent.keyDown(document, { key: 'Escape' });
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).toBeNull();
    });

    fireEvent.keyDown(document, { key: 'g' });
    fireEvent.keyDown(document, { key: 'b' });
    expect(screen.getByTestId('location').textContent).toBe('/boards');

    fireEvent.keyDown(document, { key: 'g' });
    fireEvent.keyDown(document, { key: 'u' });
    expect(screen.getByTestId('location').textContent).toBe('/settings/users');
  });

  it('uses a saved keybinding in the help overlay and the palette', async () => {
    publishKeybindings({
      'palette.open': [{ kind: 'chord', chord: { key: 'p', mod: true } }],
      'ticket.create': [],
    });
    renderHelp();

    fireEvent.keyDown(document, { key: 'k', ctrlKey: true });
    expect(screen.queryByPlaceholderText('Search actions, tickets, and agents…')).toBeNull();
    fireEvent.keyDown(document, { key: 'p', ctrlKey: true });
    expect(screen.getByPlaceholderText('Search actions, tickets, and agents…')).toBeVisible();
    expect(screen.getByText('Ctrl+P')).toBeVisible();

    fireEvent.keyDown(document, { key: 'Escape' });
    await waitFor(() => {
      expect(screen.queryByPlaceholderText('Search actions, tickets, and agents…')).toBeNull();
    });
    fireEvent.keyDown(document, { key: '?' });
    expect(screen.getByRole('dialog', { name: 'Keyboard shortcuts' })).toBeVisible();
    expect(screen.getByText('Ctrl+P')).toBeVisible();
    expect(screen.getByText('Create ticket').closest('li')).toHaveTextContent(/^Create ticket$/);
    expect(screen.getByText('Change shortcuts in Settings.')).toBeVisible();
  });

  it('omits Users on the desktop app and keeps Settings', () => {
    renderHelp({ ...adminSession, desktopMode: true });

    fireEvent.keyDown(document, { key: '?' });
    expect(screen.getByText('Go to Settings')).toBeVisible();
    expect(screen.queryByText('Go to Users')).toBeNull();

    fireEvent.keyDown(document, { key: 'Escape' });
    fireEvent.keyDown(document, { key: 'g' });
    fireEvent.keyDown(document, { key: 'u' });
    expect(screen.getByTestId('location').textContent).toBe('/boards/board-1');

    fireEvent.keyDown(document, { key: 'g' });
    fireEvent.keyDown(document, { key: 's' });
    expect(screen.getByTestId('location').textContent).toBe('/settings');
  });
});
