import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
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

describe('AppShell', () => {
  beforeEach(() => {
    localStorage.clear();
    sessionState.desktopMode = false;
  });

  it('keeps notification and sign-out controls in the same visual and focus order', () => {
    render(
      <MemoryRouter>
        <AppShell />
      </MemoryRouter>,
    );

    const bell = screen.getByRole('button', { name: 'Notifications' });
    const signOut = screen.getByRole('button', { name: 'Sign out' });

    expect(
      signOut.compareDocumentPosition(bell) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(bell.parentElement).not.toHaveClass('order-last');
  });

  it('exposes Chat in the main nav', () => {
    render(
      <MemoryRouter>
        <AppShell />
      </MemoryRouter>,
    );

    expect(screen.getByRole('link', { name: 'Chat' })).toHaveAttribute(
      'href',
      '/chat',
    );
  });

  it('uses full-width main on non-chat routes', () => {
    render(
      <MemoryRouter initialEntries={['/projects']}>
        <AppShell />
      </MemoryRouter>,
    );

    const main = screen.getByTestId('app-shell-main');
    expect(main).toHaveAttribute('data-layout', 'default');
    expect(main).toHaveClass('w-full');
    expect(main).toHaveClass('px-8');
    expect(main).not.toHaveClass('max-w-6xl');
    expect(main).not.toHaveClass('mx-auto');
    expect(main).not.toHaveAttribute('data-layout', 'chat');
  });

  it('aligns sidebar brand and top bar heights', () => {
    render(
      <MemoryRouter initialEntries={['/projects']}>
        <AppShell />
      </MemoryRouter>,
    );

    const brand = screen.getByTestId('app-shell-sidebar-brand');
    const topbar = screen.getByTestId('app-shell-topbar');
    expect(brand).toHaveClass('h-14');
    expect(topbar).toHaveClass('h-14');
  });

  it('uses full-bleed main on /chat', () => {
    render(
      <MemoryRouter initialEntries={['/chat']}>
        <AppShell />
      </MemoryRouter>,
    );

    const main = screen.getByTestId('app-shell-main');
    expect(main).toHaveAttribute('data-layout', 'chat');
    expect(main).not.toHaveClass('max-w-6xl');
    expect(main).toHaveClass('flex-1');
  });

  it('uses full-bleed main on /chat/:sessionId', () => {
    render(
      <MemoryRouter
        initialEntries={['/chat/00000000-0000-4000-8000-000000000001']}
      >
        <AppShell />
      </MemoryRouter>,
    );

    const main = screen.getByTestId('app-shell-main');
    expect(main).toHaveAttribute('data-layout', 'chat');
    expect(main).not.toHaveClass('max-w-6xl');
  });

  it('clamps chat routes to the viewport so document scroll cannot move chrome', () => {
    render(
      <MemoryRouter initialEntries={['/chat']}>
        <AppShell />
      </MemoryRouter>,
    );

    const main = screen.getByTestId('app-shell-main');
    const root = screen.getByTestId('app-shell-root');
    expect(root).toHaveClass('h-svh');
    expect(root).toHaveClass('overflow-hidden');
    expect(root).toHaveClass('flex');
    expect(main.parentElement).toHaveClass('flex-col');
    expect(main).toHaveClass('overflow-hidden');
    expect(main).toHaveClass('min-h-0');
  });

  it('does not viewport-clamp non-chat routes', () => {
    render(
      <MemoryRouter initialEntries={['/projects']}>
        <AppShell />
      </MemoryRouter>,
    );

    const root = screen.getByTestId('app-shell-root');
    expect(root).toHaveClass('min-h-screen');
    expect(root).not.toHaveClass('h-svh');
    expect(root).not.toHaveClass('overflow-hidden');
  });

  it('uses full-width main on board routes without chat viewport clamp', () => {
    render(
      <MemoryRouter
        initialEntries={['/projects/00000000-0000-4000-8000-000000000001/board']}
      >
        <AppShell />
      </MemoryRouter>,
    );

    const main = screen.getByTestId('app-shell-main');
    const root = screen.getByTestId('app-shell-root');
    expect(main).toHaveAttribute('data-layout', 'board');
    expect(main).not.toHaveClass('max-w-6xl');
    expect(main).not.toHaveClass('mx-auto');
    expect(main).toHaveClass('px-8');
    expect(main).toHaveClass('py-8');
    expect(main).not.toHaveClass('flex-1');
    expect(main).not.toHaveClass('overflow-hidden');
    expect(root).toHaveClass('min-h-screen');
    expect(root).not.toHaveClass('h-svh');
    expect(root).not.toHaveClass('overflow-hidden');
  });

  it('uses full-width main on non-board project routes', () => {
    render(
      <MemoryRouter
        initialEntries={['/projects/00000000-0000-4000-8000-000000000001']}
      >
        <AppShell />
      </MemoryRouter>,
    );

    const main = screen.getByTestId('app-shell-main');
    expect(main).toHaveAttribute('data-layout', 'default');
    expect(main).toHaveClass('w-full');
    expect(main).not.toHaveClass('max-w-6xl');
  });

  it('renders an expanded sidebar by default with nav icons', () => {
    render(
      <MemoryRouter>
        <AppShell />
      </MemoryRouter>,
    );

    const sidebar = screen.getByTestId('app-shell-sidebar');
    expect(sidebar).toHaveAttribute('data-collapsed', 'false');
    expect(screen.getByRole('link', { name: 'Projects' })).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Collapse sidebar' }),
    ).toBeInTheDocument();
  });

  it('collapses the sidebar when the toggle is used', () => {
    render(
      <MemoryRouter>
        <AppShell />
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Collapse sidebar' }));

    const sidebar = screen.getByTestId('app-shell-sidebar');
    expect(sidebar).toHaveAttribute('data-collapsed', 'true');
    expect(
      screen.getByRole('button', { name: 'Expand sidebar' }),
    ).toBeInTheDocument();
  });

  it('hides account chrome and Users nav in desktop mode', () => {
    sessionState.desktopMode = true;

    render(
      <MemoryRouter initialEntries={['/projects']}>
        <AppShell />
      </MemoryRouter>,
    );

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
    expect(screen.getByRole('link', { name: 'Tools' })).toBeInTheDocument();
  });
});
