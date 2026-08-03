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
});
