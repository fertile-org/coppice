import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { NotificationBell } from './NotificationBell';
import {
  notificationListQueryKey,
  unreadNotificationCountQueryKey,
  type NotificationItem,
} from './useNotifications';

const unreadNotification: NotificationItem = {
  id: '00000000-0000-0000-0000-000000000001',
  type: 'agent_run_finished',
  title: 'Frontend Agent run succeeded',
  body: 'Add notification hub',
  ticketId: '00000000-0000-0000-0000-000000000010',
  runId: '00000000-0000-0000-0000-000000000020',
  agentId: '00000000-0000-0000-0000-000000000030',
  commentId: null,
  mentionId: null,
  readAt: null,
  createdAt: '2026-08-03T12:00:00.000Z',
};

const readNotification: NotificationItem = {
  ...unreadNotification,
  id: '00000000-0000-0000-0000-000000000002',
  title: 'Backend Agent mentioned on ticket',
  type: 'agent_mentioned',
  readAt: '2026-08-02T12:30:00.000Z',
  createdAt: '2026-08-02T12:00:00.000Z',
};

function jsonResponse(body: unknown) {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  });
}

function mockNotificationApi(items: NotificationItem[], count: number) {
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const path = String(input);
    if (init?.method === 'POST' && path.endsWith('/mark-all-read')) {
      return jsonResponse({ marked: count });
    }
    if (init?.method === 'POST' && path.endsWith('/read')) {
      return new Response(null, { status: 204 });
    }
    if (path.endsWith('/unread-count')) {
      return jsonResponse({ count: 0 });
    }
    if (path.startsWith('/api/notifications?')) {
      return jsonResponse({ items, nextCursor: null });
    }
    throw new Error(`Unexpected request: ${path}`);
  });
}

function renderBell({
  items = [unreadNotification, readNotification],
  count = 1,
  onOpenTicket = vi.fn(),
  primeList = true,
}: {
  items?: NotificationItem[];
  count?: number;
  onOpenTicket?: (ticketId: string) => void | Promise<void>;
  primeList?: boolean;
} = {}) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: Number.POSITIVE_INFINITY },
      mutations: { retry: false },
    },
  });
  queryClient.setQueryData(unreadNotificationCountQueryKey, { count });
  if (primeList) {
    queryClient.setQueryData(notificationListQueryKey('all', 20), {
      items,
      nextCursor: null,
    });
  }

  return {
    queryClient,
    onOpenTicket,
    ...render(
      <QueryClientProvider client={queryClient}>
        <NotificationBell onOpenTicket={onOpenTicket} />
      </QueryClientProvider>,
    ),
  };
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('NotificationBell', () => {
  it('shows an unread badge and exposes the count in its accessible name', () => {
    renderBell({ count: 3 });

    const button = screen.getByRole('button', {
      name: 'Notifications, 3 unread',
    });
    expect(button).toBeInTheDocument();
    expect(within(button).getByText('3')).toBeInTheDocument();
  });

  it('opens a newest-first list with unread items visually distinguished', () => {
    renderBell({ items: [readNotification, unreadNotification] });

    fireEvent.click(
      screen.getByRole('button', { name: 'Notifications, 1 unread' }),
    );

    const list = screen.getByRole('list', { name: 'Notification history' });
    const items = within(list).getAllByRole('button');
    expect(items[0]).toHaveTextContent(unreadNotification.title);
    expect(items[1]).toHaveTextContent(readNotification.title);
    expect(items[0]).toHaveAttribute('data-unread', 'true');
    expect(items[1]).toHaveAttribute('data-unread', 'false');
  });

  it('marks an unread notification read and opens its ticket', async () => {
    const fetchMock = mockNotificationApi([readNotification], 1);
    vi.stubGlobal('fetch', fetchMock);
    const onOpenTicket = vi.fn();
    renderBell({ items: [unreadNotification], onOpenTicket });

    fireEvent.click(
      screen.getByRole('button', { name: 'Notifications, 1 unread' }),
    );
    fireEvent.click(
      screen.getByRole('button', { name: /Frontend Agent run succeeded/ }),
    );

    expect(onOpenTicket).toHaveBeenCalledWith(unreadNotification.ticketId);
    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith(
        `/api/notifications/${unreadNotification.id}/read`,
        expect.objectContaining({ method: 'POST' }),
      );
    });
  });

  it('marks all notifications read and clears the badge immediately', async () => {
    const fetchMock = mockNotificationApi(
      [
        { ...unreadNotification, readAt: '2026-08-03T12:01:00.000Z' },
        readNotification,
      ],
      2,
    );
    vi.stubGlobal('fetch', fetchMock);
    renderBell({
      items: [unreadNotification, { ...readNotification, readAt: null }],
      count: 2,
    });

    fireEvent.click(
      screen.getByRole('button', { name: 'Notifications, 2 unread' }),
    );
    fireEvent.click(
      screen.getByRole('button', { name: 'Mark all notifications as read' }),
    );

    await waitFor(() => {
      expect(
        screen.getByRole('button', {
          name: 'Notifications, no unread notifications',
        }),
      ).toBeInTheDocument();
    });
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/notifications/mark-all-read',
      expect.objectContaining({ method: 'POST' }),
    );
  });

  it('shows the empty state when there is no history', () => {
    renderBell({ items: [], count: 0 });

    fireEvent.click(
      screen.getByRole('button', {
        name: 'Notifications, no unread notifications',
      }),
    );

    expect(screen.getByText('No notifications yet')).toBeInTheDocument();
  });

  it('shows loading and error states for the notification list', async () => {
    const fetchMock = vi.fn(() => Promise.reject(new Error('offline')));
    vi.stubGlobal('fetch', fetchMock);
    renderBell({ count: 0, primeList: false });

    fireEvent.click(
      screen.getByRole('button', {
        name: 'Notifications, no unread notifications',
      }),
    );

    expect(screen.getByRole('status')).toHaveTextContent(
      'Loading notifications…',
    );
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Notifications could not be loaded.',
    );
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
  });

  it('closes on Escape and returns focus to the bell', () => {
    renderBell();
    const trigger = screen.getByRole('button', {
      name: 'Notifications, 1 unread',
    });
    fireEvent.click(trigger);
    expect(screen.getByRole('dialog', { name: 'Notifications' })).toBeInTheDocument();

    fireEvent.keyDown(document, { key: 'Escape' });

    expect(screen.queryByRole('dialog', { name: 'Notifications' })).toBeNull();
    expect(trigger).toHaveFocus();
  });
});
