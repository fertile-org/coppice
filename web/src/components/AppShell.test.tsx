import '@testing-library/jest-dom/vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { describe, expect, it, vi } from 'vitest';
import { AppShell } from './AppShell';

vi.mock('../features/auth/useSession', () => ({
  useSession: () => ({
    user: {
      id: '00000000-0000-0000-0000-000000000001',
      email: 'admin@localhost',
      role: 'admin',
    },
    logout: vi.fn(),
  }),
}));

vi.mock('../features/notifications/NotificationBell', () => ({
  NotificationBell: () => <button type="button">Notifications</button>,
}));

vi.mock('../features/tickets/useOpenTicket', () => ({
  useOpenTicket: () => vi.fn(),
}));

describe('AppShell', () => {
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

  it('keeps the default max-width main on non-chat routes', () => {
    render(
      <MemoryRouter initialEntries={['/projects']}>
        <AppShell />
      </MemoryRouter>,
    );

    const main = screen.getByTestId('app-shell-main');
    expect(main).toHaveAttribute('data-layout', 'default');
    expect(main).toHaveClass('max-w-6xl');
    expect(main).not.toHaveAttribute('data-layout', 'chat');
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
});
