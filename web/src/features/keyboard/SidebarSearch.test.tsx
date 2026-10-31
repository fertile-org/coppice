import '@testing-library/jest-dom/vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { KeyboardProvider } from './KeyboardProvider';
import { publishKeybindings } from './keybindings';
import { detectPlatform } from './platform';
import { SidebarSearch } from './SidebarSearch';

vi.mock('./platform', async () => {
  const actual = await vi.importActual<typeof import('./platform')>('./platform');
  return {
    ...actual,
    detectPlatform: vi.fn(() => 'linux'),
  };
});

function renderSearch({
  collapsed = false,
  path = '/boards',
}: {
  collapsed?: boolean;
  path?: string;
} = {}) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <KeyboardProvider>
        <SidebarSearch collapsed={collapsed} />
      </KeyboardProvider>
    </MemoryRouter>,
  );
}

describe('SidebarSearch', () => {
  beforeEach(() => {
    publishKeybindings({});
    vi.mocked(detectPlatform).mockReturnValue('linux');
  });

  afterEach(() => {
    act(() => {
      publishKeybindings({});
    });
  });

  it('opens the command palette with the input focused', async () => {
    renderSearch();

    fireEvent.click(screen.getByRole('button', { name: /Search/ }));

    const input = await screen.findByPlaceholderText('Search actions, tickets, and agents…');
    expect(screen.getByRole('dialog', { name: 'Command palette' })).toBeInTheDocument();
    await waitFor(() => {
      expect(input).toHaveFocus();
    });
  });

  it('returns focus to the search button when the palette closes', async () => {
    renderSearch();
    const search = screen.getByRole('button', { name: /Search/ });

    fireEvent.click(search);
    const input = await screen.findByPlaceholderText('Search actions, tickets, and agents…');
    fireEvent.keyDown(input, { key: 'Escape' });

    await waitFor(() => {
      expect(screen.queryByRole('dialog')).toBeNull();
    });
    expect(search).toHaveFocus();
  });

  it('shows Ctrl+K on Linux and Windows', () => {
    renderSearch();
    expect(screen.getByRole('button', { name: /Search/ }).querySelector('kbd')).toHaveTextContent(
      'Ctrl+K',
    );

    cleanup();
    vi.mocked(detectPlatform).mockReturnValue('windows');
    renderSearch();
    expect(screen.getByRole('button', { name: /Search/ }).querySelector('kbd')).toHaveTextContent(
      'Ctrl+K',
    );
  });

  it('shows ⌘+K on macOS', () => {
    vi.mocked(detectPlatform).mockReturnValue('macos');
    renderSearch();
    expect(screen.getByRole('button', { name: /Search/ }).querySelector('kbd')).toHaveTextContent(
      '⌘+K',
    );
  });

  it('shows a rebound palette shortcut and still opens when unbound', async () => {
    publishKeybindings({
      'palette.open': [{ kind: 'chord', chord: { key: 'p', mod: true } }],
    });
    renderSearch();
    expect(screen.getByRole('button', { name: /Search/ }).querySelector('kbd')).toHaveTextContent(
      'Ctrl+P',
    );

    act(() => {
      publishKeybindings({ 'palette.open': [] });
    });

    const search = screen.getByRole('button', { name: /Search/ });
    expect(search.querySelector('kbd')).toBeNull();

    fireEvent.click(search);
    expect(await screen.findByRole('dialog', { name: 'Command palette' })).toBeInTheDocument();
  });

  it('shows an icon-only collapsed button that opens the palette', async () => {
    renderSearch({ collapsed: true });

    const search = screen.getByRole('button', { name: 'Search' });
    expect(search).toHaveAttribute('title', 'Search (Ctrl+K)');
    expect(search).toHaveClass('justify-center', 'px-2', 'py-2.5');
    expect(search.querySelector('kbd')).toBeNull();
    expect(search).not.toHaveTextContent('Search…');

    fireEvent.click(search);
    expect(await screen.findByRole('dialog', { name: 'Command palette' })).toBeInTheDocument();
  });
});
