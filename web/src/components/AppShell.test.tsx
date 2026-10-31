import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ThemeProvider } from '../features/theme/ThemeProvider';
import { AppShell } from './AppShell';

const sessionState = {
  user: {
    id: '00000000-0000-0000-0000-000000000001',
    email: 'admin@localhost',
    role: 'admin',
  },
  desktopMode: false,
  logout: vi.fn(),
};

vi.mock('../features/auth/useSession', () => ({
  useSession: () => sessionState,
}));

vi.mock('../features/notifications/NotificationBell', () => ({
  NotificationBell: () => <button type="button">Notifications</button>,
}));

vi.mock('../features/tickets/useOpenTicket', () => ({
  useOpenTicket: () => vi.fn(),
}));

function renderShell(initialEntries: string[] = ['/']) {
  return render(
    <ThemeProvider>
      <MemoryRouter initialEntries={initialEntries}>
        <AppShell />
      </MemoryRouter>
    </ThemeProvider>,
  );
}

describe('AppShell', () => {
  beforeEach(() => {
    localStorage.clear();
    sessionState.desktopMode = false;
    sessionState.user = { ...sessionState.user, role: 'admin' };
    document.documentElement.removeAttribute('data-theme');
  });

  it('keeps notification and sign-out controls in the same visual and focus order', () => {
    renderShell();

    const bell = screen.getByRole('button', { name: 'Notifications' });
    const signOut = screen.getByRole('button', { name: 'Sign out' });

    expect(
      signOut.compareDocumentPosition(bell) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(bell.parentElement).not.toHaveClass('order-last');
  });

  it('exposes a theme toggle that cycles preference', () => {
    renderShell();

    const toggle = screen.getByTestId('theme-toggle');
    expect(toggle).toHaveAttribute('data-theme-preference', 'system');

    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute('data-theme-preference', 'light');
    expect(document.documentElement).toHaveAttribute('data-theme', 'light');

    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute('data-theme-preference', 'dark');
    expect(document.documentElement).toHaveAttribute('data-theme', 'dark');
  });

  it('exposes Chat in the main nav', () => {
    renderShell();

    expect(screen.getByRole('link', { name: 'Chat' })).toHaveAttribute(
      'href',
      '/chat',
    );
  });

  it('puts Skip to board first on a board route', () => {
    renderShell(['/boards/board-1']);

    const skip = screen.getByRole('link', { name: 'Skip to board' });
    const sidebar = screen.getByTestId('app-shell-sidebar');
    expect(
      skip.compareDocumentPosition(sidebar) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(skip).toHaveClass('keyboard-focus');
    expect(skip).toHaveAttribute('href', '#board');
  });

  it('does not show Skip to board away from a board', () => {
    renderShell(['/boards']);
    expect(screen.queryByRole('link', { name: 'Skip to board' })).not.toBeInTheDocument();
  });

  it('uses full-width main on non-chat routes', () => {
    renderShell(['/boards']);

    const main = screen.getByTestId('app-shell-main');
    expect(main).toHaveAttribute('data-layout', 'default');
    expect(main).toHaveClass('w-full');
    expect(main).toHaveClass('px-8');
    expect(main).not.toHaveClass('max-w-6xl');
    expect(main).not.toHaveClass('mx-auto');
    expect(main).not.toHaveAttribute('data-layout', 'chat');
  });

  it('aligns sidebar brand and top bar heights', () => {
    renderShell(['/boards']);

    const brand = screen.getByTestId('app-shell-sidebar-brand');
    const topbar = screen.getByTestId('app-shell-topbar');
    expect(brand).toHaveClass('h-14');
    expect(topbar).toHaveClass('h-14');
  });

  it('uses full-bleed main on /chat', () => {
    renderShell(['/chat']);

    const main = screen.getByTestId('app-shell-main');
    expect(main).toHaveAttribute('data-layout', 'chat');
    expect(main).not.toHaveClass('max-w-6xl');
    expect(main).toHaveClass('flex-1');
  });

  it('uses full-bleed main on /chat/:sessionId', () => {
    renderShell(['/chat/00000000-0000-4000-8000-000000000001']);

    const main = screen.getByTestId('app-shell-main');
    expect(main).toHaveAttribute('data-layout', 'chat');
    expect(main).not.toHaveClass('max-w-6xl');
  });

  it('clamps chat routes to the viewport so document scroll cannot move chrome', () => {
    renderShell(['/chat']);

    const main = screen.getByTestId('app-shell-main');
    const root = screen.getByTestId('app-shell-root');
    expect(root).toHaveClass('h-svh');
    expect(root).toHaveClass('overflow-hidden');
    expect(root).toHaveClass('flex');
    expect(main.parentElement).toHaveClass('flex-col');
    expect(main).toHaveClass('overflow-hidden');
    expect(main).toHaveClass('min-h-0');
  });

  it('scrolls non-chat pages inside main, not the window', () => {
    renderShell(['/boards']);

    const root = screen.getByTestId('app-shell-root');
    const main = screen.getByTestId('app-shell-main');
    expect(root).toHaveClass('h-svh');
    expect(root).toHaveClass('overflow-hidden');
    expect(main).toHaveClass('overflow-y-auto');
    expect(main).toHaveClass('min-h-0');
    expect(main).toHaveClass('flex-1');
  });

  it('uses full-width scrolling main on board routes', () => {
    renderShell(['/boards/00000000-0000-4000-8000-000000000001']);

    const main = screen.getByTestId('app-shell-main');
    const root = screen.getByTestId('app-shell-root');
    expect(main).toHaveAttribute('data-layout', 'board');
    expect(main).not.toHaveClass('max-w-6xl');
    expect(main).not.toHaveClass('mx-auto');
    expect(main).toHaveClass('px-8');
    expect(main).toHaveClass('py-8');
    expect(main).toHaveClass('overflow-y-auto');
    expect(main).not.toHaveClass('overflow-hidden');
    expect(root).toHaveClass('h-svh');
  });

  it('uses full-width main on non-board routes nested under a board', () => {
    renderShell(['/boards/00000000-0000-4000-8000-000000000001/runs']);

    const main = screen.getByTestId('app-shell-main');
    expect(main).toHaveAttribute('data-layout', 'default');
    expect(main).toHaveClass('w-full');
    expect(main).not.toHaveClass('max-w-6xl');
  });

  it('renders an expanded sidebar by default with nav icons', () => {
    renderShell();

    const sidebar = screen.getByTestId('app-shell-sidebar');
    expect(sidebar).toHaveAttribute('data-collapsed', 'false');
    expect(screen.getByRole('link', { name: 'Boards' })).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Collapse sidebar' }),
    ).toBeInTheDocument();
  });

  it('pins the search box above Boards, outside the scrolling nav', () => {
    renderShell();

    const brand = screen.getByTestId('app-shell-sidebar-brand');
    const search = screen.getByRole('button', { name: /Search/ });
    const boards = screen.getByRole('link', { name: 'Boards' });
    const nav = screen.getByRole('navigation', { name: 'Main' });

    expect(nav.contains(search)).toBe(false);
    expect(
      brand.compareDocumentPosition(search) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      search.compareDocumentPosition(boards) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it('collapses the sidebar when the toggle is used', () => {
    renderShell();

    fireEvent.click(screen.getByRole('button', { name: 'Collapse sidebar' }));

    const sidebar = screen.getByTestId('app-shell-sidebar');
    expect(sidebar).toHaveAttribute('data-collapsed', 'true');
    expect(
      screen.getByRole('button', { name: 'Expand sidebar' }),
    ).toBeInTheDocument();
  });

  it('hides account chrome and Users nav in desktop mode', () => {
    sessionState.desktopMode = true;

    renderShell(['/boards']);

    expect(screen.queryByText('admin@localhost')).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Sign out' }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole('link', { name: 'Users' }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Notifications' }),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'System' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Settings' })).toHaveAttribute(
      'href',
      '/settings',
    );
    expect(screen.getByTestId('theme-toggle')).toBeInTheDocument();
  });

  it('groups Backup, Connectors, and Logs under System', () => {
    renderShell(['/boards']);

    const toggle = screen.getByRole('button', { name: 'System' });
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByRole('link', { name: 'Logs' })).not.toBeInTheDocument();

    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByRole('link', { name: 'Backup' })).toHaveAttribute('href', '/system/backup');
    expect(screen.getByRole('link', { name: 'Connectors' })).toHaveAttribute(
      'href',
      '/system/connectors',
    );
    expect(screen.getByRole('link', { name: 'Logs' })).toHaveAttribute('href', '/system/logs');
  });

  it('lists Updates under System only inside the desktop app', () => {
    renderShell(['/system/logs']);
    expect(screen.queryByRole('link', { name: 'Updates' })).not.toBeInTheDocument();
    cleanup();

    window.coppiceDesktop = { pickDirectory: vi.fn(async () => null), platform: 'linux' };
    try {
      renderShell(['/system/logs']);
      expect(screen.getByRole('link', { name: 'Updates' })).toHaveAttribute(
        'href',
        '/system/updates',
      );
    } finally {
      delete window.coppiceDesktop;
    }
  });

  it('opens the System group on a System page', () => {
    renderShell(['/system/logs']);

    expect(screen.getByRole('button', { name: 'System' })).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByRole('link', { name: 'Logs' })).toHaveAttribute('aria-current', 'page');
  });

  it('highlights Settings only on its own page', () => {
    renderShell(['/settings/plugins']);

    expect(screen.getByRole('link', { name: 'Plugins' })).toHaveAttribute('aria-current', 'page');
    expect(screen.getByRole('link', { name: 'Settings' })).not.toHaveAttribute(
      'aria-current',
      'page',
    );
  });

  it('hides Settings from members', () => {
    sessionState.user = { ...sessionState.user, role: 'member' };

    renderShell(['/boards']);

    expect(screen.queryByRole('link', { name: 'Settings' })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Logs' })).not.toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Boards' })).toBeInTheDocument();
  });

  it('places Help directly below Settings for an admin', () => {
    renderShell(['/boards']);

    const nav = screen.getByRole('navigation', { name: 'Main' });
    const links = within(nav).getAllByRole('link');
    const settingsIndex = links.findIndex((link) => link.getAttribute('href') === '/settings');
    const helpIndex = links.findIndex(
      (link) => link.getAttribute('href') === 'https://getcoppice.app/docs',
    );

    expect(settingsIndex).toBeGreaterThanOrEqual(0);
    expect(helpIndex).toBe(settingsIndex + 1);
  });

  it('shows Help as the last nav item for a member', () => {
    sessionState.user = { ...sessionState.user, role: 'member' };

    renderShell(['/boards']);

    expect(screen.queryByRole('link', { name: 'Settings' })).not.toBeInTheDocument();

    const nav = screen.getByRole('navigation', { name: 'Main' });
    const links = within(nav).getAllByRole('link');
    const last = links[links.length - 1];
    expect(last).toHaveAccessibleName('Help');
    expect(last).toHaveAttribute('href', 'https://getcoppice.app/docs');
  });

  it('opens Help in a new tab to the public docs', () => {
    renderShell(['/boards']);

    const help = screen.getByRole('link', { name: 'Help' });
    expect(help).toHaveAttribute('href', 'https://getcoppice.app/docs');
    expect(help).toHaveAttribute('target', '_blank');
    expect(help).toHaveAttribute('rel', 'noreferrer');
  });

  it('shows only the Help icon when the sidebar is collapsed', () => {
    renderShell();

    fireEvent.click(screen.getByRole('button', { name: 'Collapse sidebar' }));

    const help = screen.getByRole('link', { name: 'Help' });
    expect(help).toHaveAttribute('title', 'Help');
    expect(help).toHaveAttribute('aria-label', 'Help');
    expect(help).not.toHaveTextContent('Help');
  });

  it('never marks Help as the active nav item', () => {
    renderShell(['/settings']);

    const help = screen.getByRole('link', { name: 'Help' });
    expect(help).not.toHaveAttribute('aria-current');
    expect(help).not.toHaveClass('bg-accent-muted');
    expect(help).toHaveClass('text-text-secondary');
  });
});
