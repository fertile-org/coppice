import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useContext, useEffect } from 'react';
import { MemoryRouter, useLocation } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SessionContext, type SessionContextValue } from '../auth/useSession';
import { KeyboardContext } from './keyboardContext';
import { KeyboardProvider } from './KeyboardProvider';
import { publishKeybindings } from './keybindings';
import { PALETTE_COPY, PALETTE_HELP_LABEL, PALETTE_RECENT_KEY } from './palette';

const adminSession: SessionContextValue = {
  user: { id: 'user-1', email: 'ada@example.com', role: 'admin' },
  loading: false,
  desktopMode: false,
  establishSession: () => undefined,
  logout: async () => undefined,
};

function LocationProbe() {
  const location = useLocation();
  return (
    <div data-testid="location">
      {location.pathname}
      {location.search}
    </div>
  );
}

function EnableColumnJumps({ onRun }: { onRun: (id: string) => void }) {
  const keyboard = useContext(KeyboardContext);
  useEffect(() => {
    if (!keyboard) return;
    const cleanups = [1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) =>
      keyboard.register(`board.column.${n}`, {
        run: () => onRun(`board.column.${n}`),
        enabled: () => true,
      }),
    );
    return () => {
      for (const cleanup of cleanups) cleanup();
    };
  }, [keyboard, onRun]);
  return null;
}

function renderPalette({
  session = null,
  onRun = vi.fn(),
  path = '/boards/board-1',
}: {
  session?: SessionContextValue | null;
  onRun?: (id: string) => void;
  path?: string;
} = {}) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[path]}>
        <SessionContext.Provider value={session}>
          <KeyboardProvider>
            <button type="button">Before</button>
            <EnableColumnJumps onRun={onRun} />
            <LocationProbe />
          </KeyboardProvider>
        </SessionContext.Provider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

function dispatchKey(target: EventTarget, init: KeyboardEventInit) {
  const event = new KeyboardEvent('keydown', {
    bubbles: true,
    cancelable: true,
    ...init,
  });
  act(() => {
    target.dispatchEvent(event);
  });
  return event;
}

function openPalette() {
  const event = dispatchKey(document, { key: 'k', ctrlKey: true });
  expect(event.defaultPrevented).toBe(true);
  return screen.getByRole('dialog', { name: 'Command palette' });
}

describe('command palette', () => {
  beforeEach(() => {
    publishKeybindings({});
    localStorage.clear();
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo) => {
        const url = String(input);
        if (url.includes('/api/agents')) {
          return new Response(
            JSON.stringify({ items: [{ id: 'agt-1', name: 'Planner' }] }),
            { status: 200, headers: { 'Content-Type': 'application/json' } },
          );
        }
        if (url.includes('/tickets')) {
          return new Response(
            JSON.stringify([
              {
                id: 'tic-1',
                title: 'Plan the release',
                boardId: 'board-1',
              },
            ]),
            { status: 200, headers: { 'Content-Type': 'application/json' } },
          );
        }
        return new Response('{}', { status: 200 });
      }),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    document.querySelector('.xterm')?.remove();
  });

  it('opens with Ctrl+K, lists pages only, and leaves G then 2 unchanged', () => {
    const onRun = vi.fn();
    renderPalette({ onRun });

    openPalette();
    expect(screen.getByPlaceholderText(PALETTE_COPY.placeholder)).toBeInTheDocument();
    expect(screen.getByText(PALETTE_COPY.pages)).toBeInTheDocument();
    expect(screen.getByRole('option', { name: /Go to Boards/ })).toHaveTextContent('G then B');
    expect(screen.getByRole('option', { name: new RegExp(PALETTE_HELP_LABEL) })).toHaveTextContent(
      '?',
    );
    expect(screen.queryByRole('option', { name: /Go to Plan Review column/ })).toBeNull();
    expect(screen.queryByRole('option', { name: /Go to Backlog column/ })).toBeNull();
    expect(screen.queryByRole('option', { name: /Open command palette/ })).toBeNull();
    expect(screen.queryByRole('option', { name: /^Close$/ })).toBeNull();
    expect(screen.queryByRole('option', { name: /Next card/ })).toBeNull();
    expect(screen.queryByRole('option', { name: 'Plan the release' })).toBeNull();

    fireEvent.keyDown(document.activeElement ?? document.body, { key: 'Escape' });
    expect(screen.queryByRole('dialog', { name: 'Command palette' })).toBeNull();

    dispatchKey(document, { key: 'g' });
    dispatchKey(document, { key: '2' });
    expect(onRun).toHaveBeenCalledWith('board.column.2');
    expect(screen.queryByRole('dialog')).toBeNull();

    dispatchKey(document, { key: 'g' });
    dispatchKey(document, { key: 'b' });
    expect(screen.getByTestId('location').textContent).toBe('/boards');
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('omits the key hint when Keyboard shortcuts is unbound', () => {
    publishKeybindings({ 'help.show': [] });
    renderPalette();

    openPalette();
    const shortcuts = screen.getByRole('option', { name: new RegExp(PALETTE_HELP_LABEL) });
    expect(shortcuts).toHaveTextContent(PALETTE_HELP_LABEL);
    expect(shortcuts).not.toHaveTextContent('?');
    expect(screen.getByRole('option', { name: /Go to Boards/ })).toHaveTextContent('G then B');
  });

  it('filters as you type, opens the highlighted page with Enter, and restores focus on Esc', async () => {
    renderPalette();
    const before = screen.getByRole('button', { name: 'Before' });
    before.focus();
    openPalette();

    const input = screen.getByPlaceholderText(PALETTE_COPY.placeholder);
    fireEvent.change(input, { target: { value: 'go to b' } });

    const boards = screen.getByRole('option', { name: /Go to Boards/ });
    expect(boards).toBeInTheDocument();
    expect(screen.queryByRole('option', { name: /Go to Agents/ })).toBeNull();
    expect(boards).toHaveAttribute('aria-selected', 'true');
    fireEvent.keyDown(input, { key: 'Enter' });

    await waitFor(() => {
      expect(screen.getByTestId('location').textContent).toBe('/boards');
    });
    expect(screen.queryByRole('dialog')).toBeNull();

    before.focus();
    openPalette();
    fireEvent.keyDown(screen.getByPlaceholderText(PALETTE_COPY.placeholder), {
      key: 'Escape',
    });
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).toBeNull();
    });
    expect(before).toHaveFocus();
  });

  it('finds a ticket title and an agent name, then opens them', async () => {
    renderPalette();
    openPalette();
    const input = screen.getByPlaceholderText(PALETTE_COPY.placeholder);
    fireEvent.change(input, { target: { value: 'zzzz-nope' } });
    expect(screen.getByText('No matches')).toBeVisible();
    expect(screen.queryAllByRole('option')).toHaveLength(0);

    fireEvent.change(input, { target: { value: 'release' } });
    expect(await screen.findByRole('option', { name: 'Plan the release' })).toBeInTheDocument();
    expect(screen.queryByText('No matches')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('option', { name: 'Plan the release' }));
    await waitFor(() => {
      expect(screen.getByTestId('location').textContent).toBe('/boards/board-1?ticket=tic-1');
    });

    openPalette();
    fireEvent.change(screen.getByPlaceholderText(PALETTE_COPY.placeholder), {
      target: { value: 'planner' },
    });
    expect(await screen.findByRole('option', { name: 'Planner' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('option', { name: 'Planner' }));
    await waitFor(() => {
      expect(screen.getByTestId('location').textContent).toBe('/agents');
    });
  });

  it('opens Keyboard shortcuts at /help/shortcuts and keeps ? on the overlay', async () => {
    renderPalette();
    openPalette();
    fireEvent.click(screen.getByRole('option', { name: new RegExp(PALETTE_HELP_LABEL) }));
    await waitFor(() => {
      expect(screen.getByTestId('location').textContent).toBe('/help/shortcuts');
    });
    expect(screen.queryByRole('dialog', { name: 'Command palette' })).toBeNull();
    expect(screen.queryByRole('dialog', { name: 'Keyboard shortcuts' })).toBeNull();

    fireEvent.keyDown(document, { key: '?' });
    expect(screen.getByRole('dialog', { name: 'Keyboard shortcuts' })).toBeVisible();
  });

  it('toggles closed on Mod+K, including from a text field, and stays closed in the terminal', () => {
    renderPalette();
    openPalette();
    const close = dispatchKey(screen.getByPlaceholderText(PALETTE_COPY.placeholder), {
      key: 'k',
      ctrlKey: true,
    });
    expect(close.defaultPrevented).toBe(true);
    expect(screen.queryByRole('dialog')).toBeNull();

    const field = document.createElement('input');
    document.body.appendChild(field);
    field.focus();
    const fromField = dispatchKey(field, { key: 'k', ctrlKey: true });
    expect(fromField.defaultPrevented).toBe(true);
    expect(screen.getByRole('dialog', { name: 'Command palette' })).toBeInTheDocument();
    field.remove();

    fireEvent.keyDown(screen.getByPlaceholderText(PALETTE_COPY.placeholder), {
      key: 'Escape',
    });

    const terminal = document.createElement('div');
    terminal.className = 'xterm';
    const termInput = document.createElement('textarea');
    terminal.appendChild(termInput);
    document.body.appendChild(terminal);
    termInput.focus();
    const fromTerminal = dispatchKey(termInput, { key: 'k', ctrlKey: true });
    expect(fromTerminal.defaultPrevented).toBe(false);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('keeps recent pages and omits admin pages and stale Next card for members', async () => {
    localStorage.setItem(PALETTE_RECENT_KEY, JSON.stringify(['board.nextCard', 'nav.chat']));
    renderPalette();
    openPalette();
    expect(screen.queryByRole('option', { name: /Go to Plugins/ })).toBeNull();
    expect(screen.queryByRole('option', { name: /Next card/ })).toBeNull();
    expect(screen.getByText('Recent')).toBeInTheDocument();
    expect(screen.getByRole('option', { name: /Go to Chat/ })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: /Go to Boards/ })).toBeInTheDocument();

    const input = screen.getByPlaceholderText(PALETTE_COPY.placeholder);
    fireEvent.change(input, { target: { value: 'go to boards' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => {
      expect(screen.getByTestId('location').textContent).toBe('/boards');
    });
    expect(screen.queryByRole('dialog')).toBeNull();

    openPalette();
    expect(screen.getByText('Recent')).toBeInTheDocument();
    expect(screen.getByRole('option', { name: /Go to Boards/ })).toBeInTheDocument();
  });

  it('lists admin pages for an admin', () => {
    renderPalette({ session: adminSession });
    openPalette();
    expect(screen.getByRole('option', { name: /Go to Plugins/ })).toHaveTextContent('G then P');
    expect(screen.getByRole('option', { name: /Go to Users/ })).toBeInTheDocument();
  });
});
