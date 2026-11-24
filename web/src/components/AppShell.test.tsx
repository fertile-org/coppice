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

  it('lists primary sidebar links as Boards, Chat, Agents, Knowledge', () => {
    renderShell(['/boards']);

    const sidebar = screen.getByTestId('app-shell-sidebar');
    const labels = ['Boards', 'Chat', 'Agents', 'Knowledge'] as const;
    const links = within(sidebar)
      .getAllByRole('link')
      .filter((link) => labels.includes(link.textContent?.trim() as (typeof labels)[number]));
    expect(links.map((link) => link.textContent?.trim())).toEqual([...labels]);
  });

  it('puts Skip to board first on a board route', () => {
    renderShell(['/boards/board-1']);

    const skip = screen.getByRole('link', { name: 'Skip to board' });
    const sidebar = screen.getByTestId('app-shell-sidebar');
    expect(
      skip.compareDocumentPosition(sidebar) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(skip).toHaveAttribute('href', '#board');
  });

  it('does not show Skip to board away from a board', () => {
    renderShell(['/boards']);
    expect(screen.queryByRole('link', { name: 'Skip to board' })).not.toBeInTheDocument();
  });

  it('uses chat and default main layouts by route', () => {
    renderShell(['/chat']);
    let main = screen.getByTestId('app-shell-main');
    expect(main).toHaveAttribute('data-layout', 'chat');
    expect(main).toHaveClass('overflow-hidden');
    cleanup();

    renderShell(['/boards/00000000-0000-4000-8000-000000000001/runs']);
    main = screen.getByTestId('app-shell-main');
    expect(main).toHaveAttribute('data-layout', 'default');
    expect(main).toHaveClass('overflow-y-auto');
  });

  it('uses board main layout on a board route', () => {
    renderShell(['/boards/00000000-0000-4000-8000-000000000001']);

    const main = screen.getByTestId('app-shell-main');
    expect(main).toHaveAttribute('data-layout', 'board');
    expect(main).toHaveClass('overflow-y-auto');
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

  it('hides Settings from members', () => {
    sessionState.user = { ...sessionState.user, role: 'member' };

    renderShell(['/boards']);

    expect(screen.queryByRole('link', { name: 'Settings' })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Logs' })).not.toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Boards' })).toBeInTheDocument();
  });

  it('opens Documentation in a new tab and never marks it active', () => {
    renderShell(['/help/shortcuts']);

    const docs = screen.getByRole('link', { name: 'Documentation' });
    expect(docs).toHaveAttribute('href', 'https://getcoppice.app/docs');
    expect(docs).toHaveAttribute('target', '_blank');
    expect(docs).toHaveAttribute('rel', 'noreferrer');
    expect(docs).not.toHaveAttribute('aria-current');
    expect(screen.getByRole('link', { name: 'Keyboard shortcuts' })).toHaveAttribute(
      'aria-current',
      'page',
    );
    expect(screen.getByRole('button', { name: 'Help' })).toHaveAttribute('aria-expanded', 'true');
  });

});
