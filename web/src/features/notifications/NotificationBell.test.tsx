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

const USER_ID = '00000000-0000-0000-0000-000000000100';

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
  primeCount = true,
  userId = USER_ID,
}: {
  items?: NotificationItem[];
  count?: number;
  onOpenTicket?: (ticketId: string) => void | Promise<void>;
  primeList?: boolean;
  primeCount?: boolean;
  userId?: string;
} = {}) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: Number.POSITIVE_INFINITY },
      mutations: { retry: false },
    },
  });
  if (primeCount) {
    queryClient.setQueryData(unreadNotificationCountQueryKey(userId), { count });
  }
  if (primeList) {
    queryClient.setQueryData(notificationListQueryKey(userId, 'all', 20), {
      items,
      nextCursor: null,
    });
  }

  return {
    queryClient,
    onOpenTicket,
    ...render(
      <QueryClientProvider client={queryClient}>
        <NotificationBell userId={userId} onOpenTicket={onOpenTicket} />
      </QueryClientProvider>,
    ),
  };
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('NotificationBell', () => {
  it('does not reuse notification state when the signed-in user changes', async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      if (String(input).endsWith('/unread-count')) {
        return jsonResponse({ count: 0 });
      }
      throw new Error(`Unexpected request: ${String(input)}`);
    });
    vi.stubGlobal('fetch', fetchMock);
    const firstUserId = '00000000-0000-0000-0000-000000000301';
    const secondUserId = '00000000-0000-0000-0000-000000000302';
    const { queryClient, onOpenTicket, rerender } = renderBell({
      userId: firstUserId,
      count: 6,
    });
    expect(
      screen.getByRole('button', { name: 'Notifications, 6 unread' }),
    ).toBeInTheDocument();

    rerender(
      <QueryClientProvider client={queryClient}>
        <NotificationBell
          userId={secondUserId}
          onOpenTicket={onOpenTicket}
        />
      </QueryClientProvider>,
    );

    expect(
      screen.queryByRole('button', { name: 'Notifications, 6 unread' }),
    ).toBeNull();
    expect(
      await screen.findByRole('button', {
        name: 'Notifications, no unread notifications',
      }),
    ).toBeInTheDocument();
  });

  it('shows an unread badge and exposes the count in its accessible name', () => {
    renderBell({ count: 3 });

    const button = screen.getByRole('button', {
      name: 'Notifications, 3 unread',
    });
    expect(button).toBeInTheDocument();
    expect(within(button).getByText('3')).toBeInTheDocument();
  });

  it('moves focus into the notification dialog when it opens', () => {
    renderBell();

    fireEvent.click(
      screen.getByRole('button', { name: 'Notifications, 1 unread' }),
    );

    expect(
      screen.getByRole('heading', { name: 'Notifications' }),
    ).toHaveFocus();
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
    expect(items[0]).toHaveAccessibleName(
      expect.stringMatching(/^Unread notification\./),
    );
    expect(items[1]).toHaveAccessibleName(
      expect.stringMatching(/^Read notification\./),
    );
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

    await waitFor(() => {
      expect(onOpenTicket).toHaveBeenCalledWith(unreadNotification.ticketId);
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
    const { queryClient } = renderBell({
      items: [unreadNotification, { ...readNotification, readAt: null }],
      count: 2,
    });
    const otherUserId = '00000000-0000-0000-0000-000000000200';
    queryClient.setQueryData(unreadNotificationCountQueryKey(otherUserId), {
      count: 7,
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
    expect(
      queryClient.getQueryData(unreadNotificationCountQueryKey(otherUserId)),
    ).toEqual({ count: 7 });
    expect(
      screen.getByRole('heading', { name: 'Notifications' }),
    ).toHaveFocus();
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

  it('uses a neutral accessible label and retry state when count loading fails', async () => {
    vi.stubGlobal('fetch', vi.fn(() => Promise.reject(new Error('offline'))));
    renderBell({ items: [], primeCount: false });

    expect(
      screen.getByRole('button', {
        name: 'Notifications, loading unread count',
      }),
    ).toBeInTheDocument();
    const trigger = await screen.findByRole('button', {
      name: 'Notifications, unread count unavailable',
    });
    fireEvent.click(trigger);

    expect(screen.getByText('Unread count unavailable.')).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Retry count' }),
    ).toBeInTheDocument();
  });

  it('surfaces ticket navigation failures instead of rejecting silently', async () => {
    const fetchMock = mockNotificationApi([unreadNotification], 1);
    vi.stubGlobal('fetch', fetchMock);
    renderBell({
      items: [unreadNotification],
      onOpenTicket: vi.fn(() => Promise.reject(new Error('ticket unavailable'))),
    });

    fireEvent.click(
      screen.getByRole('button', { name: 'Notifications, 1 unread' }),
    );
    fireEvent.click(
      screen.getByRole('button', { name: /Frontend Agent run succeeded/ }),
    );

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Unable to open the related ticket.',
    );
  });
});
