import {
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';

export interface NotificationItem {
  id: string;
  type: string;
  title: string;
  body: string | null;
  ticketId: string | null;
  runId: string | null;
  agentId: string | null;
  commentId: string | null;
  mentionId: string | null;
  readAt: string | null;
  createdAt: string;
}

export interface NotificationPage {
  items: NotificationItem[];
  nextCursor: string | null;
}

interface UnreadCountResponse {
  count: number;
}

interface MarkAllReadResponse {
  marked: number;
}

export const NOTIFICATIONS_QUERY_KEY = ['notifications'] as const;
export function notificationUserQueryKey(userId: string) {
  return [...NOTIFICATIONS_QUERY_KEY, 'user', userId] as const;
}

function notificationListPrefix(userId: string) {
  return [...notificationUserQueryKey(userId), 'list'] as const;
}

export function unreadNotificationCountQueryKey(userId: string) {
  return [...notificationUserQueryKey(userId), 'unread-count'] as const;
}

export function notificationListQueryKey(
  userId: string,
  filter: 'all' | 'unread',
  limit: number,
) {
  return [...notificationListPrefix(userId), filter, limit] as const;
}

async function fetchNotifications(
  filter: 'all' | 'unread',
  limit: number,
): Promise<NotificationPage> {
  const params = new URLSearchParams({ filter, limit: String(limit) });
  const response = await apiFetch(`/api/notifications?${params.toString()}`);
  return response.json() as Promise<NotificationPage>;
}

async function fetchUnreadCount(): Promise<UnreadCountResponse> {
  const response = await apiFetch('/api/notifications/unread-count');
  return response.json() as Promise<UnreadCountResponse>;
}

async function markNotificationRead(notificationId: string): Promise<void> {
  await apiFetch(`/api/notifications/${notificationId}/read`, {
    method: 'POST',
  });
}

async function markAllNotificationsRead(): Promise<MarkAllReadResponse> {
  const response = await apiFetch('/api/notifications/mark-all-read', {
    method: 'POST',
  });
  return response.json() as Promise<MarkAllReadResponse>;
}

function updateNotificationLists(
  queryClient: QueryClient,
  userId: string,
  update: (item: NotificationItem) => NotificationItem,
) {
  queryClient.setQueriesData<NotificationPage>(
    { queryKey: notificationListPrefix(userId) },
    (page) =>
      page
        ? {
            ...page,
            items: page.items.map(update),
          }
        : page,
  );
}

function restoreNotificationQueries(
  queryClient: QueryClient,
  snapshots: ReturnType<QueryClient['getQueriesData']>,
) {
  for (const [queryKey, data] of snapshots) {
    queryClient.setQueryData(queryKey, data);
  }
}

export function useNotifications({
  userId,
  filter = 'all',
  limit = 20,
  enabled = true,
}: {
  userId: string;
  filter?: 'all' | 'unread';
  limit?: number;
  enabled?: boolean;
}) {
  return useQuery({
    queryKey: notificationListQueryKey(userId, filter, limit),
    queryFn: () => fetchNotifications(filter, limit),
    enabled,
  });
}

export function useUnreadNotificationCount(userId: string) {
  return useQuery({
    queryKey: unreadNotificationCountQueryKey(userId),
    queryFn: fetchUnreadCount,
  });
}

export function useMarkNotificationRead(userId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (notification: NotificationItem) =>
      markNotificationRead(notification.id),
    onMutate: async (notification) => {
      const userQueryKey = notificationUserQueryKey(userId);
      await queryClient.cancelQueries({ queryKey: userQueryKey });
      const snapshots = queryClient.getQueriesData({
        queryKey: userQueryKey,
      });

      if (notification.readAt === null) {
        const readAt = new Date().toISOString();
        updateNotificationLists(queryClient, userId, (item) =>
          item.id === notification.id ? { ...item, readAt } : item,
        );
        queryClient.setQueryData<UnreadCountResponse>(
          unreadNotificationCountQueryKey(userId),
          (current) =>
            current
              ? { count: Math.max(0, current.count - 1) }
              : current,
        );
      }

      return { snapshots };
    },
    onError: (_error, _notification, context) => {
      if (context) {
        restoreNotificationQueries(queryClient, context.snapshots);
      }
    },
    onSettled: () => {
      void queryClient.invalidateQueries({
        queryKey: notificationUserQueryKey(userId),
      });
    },
  });
}

export function useMarkAllNotificationsRead(userId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: markAllNotificationsRead,
    onMutate: async () => {
      const userQueryKey = notificationUserQueryKey(userId);
      await queryClient.cancelQueries({ queryKey: userQueryKey });
      const snapshots = queryClient.getQueriesData({
        queryKey: userQueryKey,
      });
      const readAt = new Date().toISOString();

      updateNotificationLists(queryClient, userId, (item) =>
        item.readAt === null ? { ...item, readAt } : item,
      );
      queryClient.setQueryData<UnreadCountResponse>(
        unreadNotificationCountQueryKey(userId),
        (current) => (current ? { count: 0 } : current),
      );

      return { snapshots };
    },
    onError: (_error, _variables, context) => {
      if (context) {
        restoreNotificationQueries(queryClient, context.snapshots);
      }
    },
    onSettled: () => {
      void queryClient.invalidateQueries({
        queryKey: notificationUserQueryKey(userId),
      });
    },
  });
}
