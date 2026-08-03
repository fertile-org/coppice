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
const NOTIFICATION_LIST_QUERY_KEY = [
  ...NOTIFICATIONS_QUERY_KEY,
  'list',
] as const;
export const unreadNotificationCountQueryKey = [
  ...NOTIFICATIONS_QUERY_KEY,
  'unread-count',
] as const;

export function notificationListQueryKey(filter: 'all' | 'unread', limit: number) {
  return [...NOTIFICATION_LIST_QUERY_KEY, filter, limit] as const;
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
  update: (item: NotificationItem) => NotificationItem,
) {
  queryClient.setQueriesData<NotificationPage>(
    { queryKey: NOTIFICATION_LIST_QUERY_KEY },
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
  filter = 'all',
  limit = 20,
  enabled = true,
}: {
  filter?: 'all' | 'unread';
  limit?: number;
  enabled?: boolean;
} = {}) {
  return useQuery({
    queryKey: notificationListQueryKey(filter, limit),
    queryFn: () => fetchNotifications(filter, limit),
    enabled,
  });
}

export function useUnreadNotificationCount() {
  return useQuery({
    queryKey: unreadNotificationCountQueryKey,
    queryFn: fetchUnreadCount,
  });
}

export function useMarkNotificationRead() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (notification: NotificationItem) =>
      markNotificationRead(notification.id),
    onMutate: async (notification) => {
      await queryClient.cancelQueries({ queryKey: NOTIFICATIONS_QUERY_KEY });
      const snapshots = queryClient.getQueriesData({
        queryKey: NOTIFICATIONS_QUERY_KEY,
      });

      if (notification.readAt === null) {
        const readAt = new Date().toISOString();
        updateNotificationLists(queryClient, (item) =>
          item.id === notification.id ? { ...item, readAt } : item,
        );
        queryClient.setQueryData<UnreadCountResponse>(
          unreadNotificationCountQueryKey,
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
      void queryClient.invalidateQueries({ queryKey: NOTIFICATIONS_QUERY_KEY });
    },
  });
}

export function useMarkAllNotificationsRead() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: markAllNotificationsRead,
    onMutate: async () => {
      await queryClient.cancelQueries({ queryKey: NOTIFICATIONS_QUERY_KEY });
      const snapshots = queryClient.getQueriesData({
        queryKey: NOTIFICATIONS_QUERY_KEY,
      });
      const readAt = new Date().toISOString();

      updateNotificationLists(queryClient, (item) =>
        item.readAt === null ? { ...item, readAt } : item,
      );
      queryClient.setQueryData<UnreadCountResponse>(
        unreadNotificationCountQueryKey,
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
      void queryClient.invalidateQueries({ queryKey: NOTIFICATIONS_QUERY_KEY });
    },
  });
}
