import { useEffect, useMemo, useRef, useState } from 'react';
import { Bell, CheckCheck } from 'lucide-react';
import {
  formatNotificationDateTime,
  formatNotificationTimestamp,
  notificationBellLabel,
} from './notification-format';
import {
  useMarkAllNotificationsRead,
  useMarkNotificationRead,
  useNotifications,
  useUnreadNotificationCount,
  type NotificationItem,
} from './useNotifications';

interface NotificationBellProps {
  userId: string;
  onOpenTicket: (ticketId: string) => void | Promise<void>;
}

function newestFirst(items: NotificationItem[]): NotificationItem[] {
  return [...items].sort((left, right) => {
    const rightTime = new Date(right.createdAt).getTime();
    const leftTime = new Date(left.createdAt).getTime();
    return rightTime - leftTime;
  });
}

function NotificationRow({
  notification,
  onSelect,
}: {
  notification: NotificationItem;
  onSelect: (notification: NotificationItem) => void;
}) {
  const unread = notification.readAt === null;
  const formattedDateTime = formatNotificationDateTime(notification.createdAt);

  return (
    <li>
      <button
        type="button"
        data-unread={String(unread)}
        onClick={() => onSelect(notification)}
        className={[
          'group relative flex w-full gap-3 px-4 py-3 text-left transition-colors duration-fast',
          'focus-visible:z-10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent',
          unread
            ? 'bg-moss-50 hover:bg-moss-100'
            : 'bg-surface-raised hover:bg-paper-100',
        ].join(' ')}
      >
        <span
          aria-hidden="true"
          className={[
            'mt-1.5 h-2 w-2 shrink-0 rounded-full',
            unread ? 'bg-moss-600' : 'bg-bark-200',
          ].join(' ')}
        />
        <span className="min-w-0 flex-1">
          <span className="sr-only">
            {unread ? 'Unread notification. ' : 'Read notification. '}
          </span>
          <span
            className={[
              'block font-body text-sm text-text-primary',
              unread ? 'font-semibold' : 'font-medium',
            ].join(' ')}
          >
            {notification.title}
          </span>
          {notification.body && (
            <span className="mt-0.5 line-clamp-2 block font-body text-xs leading-relaxed text-text-secondary">
              {notification.body}
            </span>
          )}
          <time
            dateTime={notification.createdAt}
            title={formattedDateTime}
            className="mt-1.5 block font-body text-xs text-text-secondary"
          >
            {formatNotificationTimestamp(notification.createdAt)}
          </time>
        </span>
      </button>
    </li>
  );
}

export function NotificationBell({ userId, onOpenTicket }: NotificationBellProps) {
  const [open, setOpen] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const popoverTitleRef = useRef<HTMLHeadingElement>(null);
  const countQuery = useUnreadNotificationCount(userId);
  const listQuery = useNotifications({ userId, enabled: open });
  const markRead = useMarkNotificationRead(userId);
  const markAllRead = useMarkAllNotificationsRead(userId);
  const unreadCount = countQuery.data?.count ?? 0;
  const notifications = useMemo(
    () => newestFirst(listQuery.data?.items ?? []),
    [listQuery.data?.items],
  );
  const hasUnreadNotifications =
    unreadCount > 0 || notifications.some((item) => item.readAt === null);
  const bellLabel = countQuery.data
    ? notificationBellLabel(unreadCount)
    : countQuery.isError
      ? 'Notifications, unread count unavailable'
      : 'Notifications, loading unread count';

  useEffect(() => {
    if (!open) return;

    popoverTitleRef.current?.focus();

    function handleMouseDown(event: MouseEvent) {
      if (!containerRef.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === 'Escape') {
        setOpen(false);
        triggerRef.current?.focus();
      }
    }

    document.addEventListener('mousedown', handleMouseDown);
    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('mousedown', handleMouseDown);
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [open]);

  function togglePopover() {
    setActionError(null);
    setOpen((current) => !current);
  }

  function handleNotificationClick(notification: NotificationItem) {
    setActionError(null);
    setOpen(false);
    if (notification.readAt === null) {
      markRead.mutate(notification, {
        onError: () => {
          setActionError('Unable to mark this notification as read. Try again.');
          setOpen(true);
        },
      });
    }
    if (notification.ticketId) {
      void Promise.resolve()
        .then(() => onOpenTicket(notification.ticketId!))
        .catch(() => {
          setActionError('Unable to open the related ticket. Try again.');
          setOpen(true);
        });
    }
  }

  async function handleMarkAllRead() {
    setActionError(null);
    try {
      const mutation = markAllRead.mutateAsync();
      popoverTitleRef.current?.focus();
      await mutation;
    } catch {
      setActionError('Unable to mark notifications as read. Try again.');
    }
  }

  return (
    <div ref={containerRef} className="relative">
      <button
        ref={triggerRef}
        type="button"
        aria-label={bellLabel}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls="notification-popover"
        onClick={togglePopover}
        className="relative flex h-11 w-11 items-center justify-center rounded-full border border-transparent text-text-secondary transition-colors duration-fast hover:border-border hover:bg-paper-100 hover:text-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-surface"
      >
        <Bell aria-hidden="true" className="h-5 w-5" strokeWidth={1.8} />
        {unreadCount > 0 && (
          <span
            aria-hidden="true"
            className="absolute -right-1 -top-1 flex h-5 min-w-5 items-center justify-center rounded-full border-2 border-surface bg-danger px-1 font-body text-[10px] font-bold leading-none text-text-inverse shadow-sm"
          >
            {unreadCount > 99 ? '99+' : unreadCount}
          </span>
        )}
      </button>

      {open && (
        <section
          id="notification-popover"
          role="dialog"
          aria-labelledby="notification-popover-title"
          className="absolute right-0 z-50 mt-2 w-[min(24rem,calc(100vw-2rem))] overflow-hidden rounded-xl border border-border bg-surface-raised shadow-lg"
        >
          <div className="flex min-h-14 items-center justify-between gap-3 border-b border-border bg-paper-50 px-4 py-3">
            <div>
              <h2
                ref={popoverTitleRef}
                id="notification-popover-title"
                tabIndex={-1}
                className="font-display text-base font-semibold text-text-primary"
              >
                Notifications
              </h2>
              <p className="font-body text-xs text-text-secondary">
                Recent workspace activity
              </p>
            </div>
            {hasUnreadNotifications && (
              <button
                type="button"
                aria-label="Mark all notifications as read"
                disabled={markAllRead.isPending}
                onClick={() => void handleMarkAllRead()}
                className="inline-flex min-h-11 items-center gap-1.5 rounded-md px-2 py-1.5 font-body text-xs font-medium text-accent transition-colors duration-fast hover:bg-accent-muted disabled:cursor-wait disabled:opacity-60"
              >
                <CheckCheck aria-hidden="true" className="h-4 w-4" />
                {markAllRead.isPending ? 'Marking…' : 'Mark all read'}
              </button>
            )}
          </div>

          {actionError && (
            <p role="alert" className="border-b border-danger/20 bg-danger-muted px-4 py-2 font-body text-xs text-danger">
              {actionError}
            </p>
          )}

          {countQuery.isError && !countQuery.data && (
            <div
              role="alert"
              className="flex min-h-11 items-center justify-between gap-3 border-b border-warning/20 bg-warning-muted px-4 py-2 font-body text-xs text-text-primary"
            >
              <span>Unread count unavailable.</span>
              <button
                type="button"
                onClick={() => void countQuery.refetch()}
                className="min-h-11 rounded-md px-2 font-medium text-text-primary underline decoration-bark-300 underline-offset-2 hover:text-accent"
              >
                Retry count
              </button>
            </div>
          )}

          {listQuery.isLoading && (
            <div
              role="status"
              className="flex min-h-40 items-center justify-center px-6 py-8 font-body text-sm text-text-secondary"
            >
              Loading notifications…
            </div>
          )}

          {listQuery.isError && (
            <div role="alert" className="px-6 py-8 text-center">
              <p className="font-body text-sm font-medium text-text-primary">
                Notifications could not be loaded.
              </p>
              <button
                type="button"
                onClick={() => void listQuery.refetch()}
                className="mt-3 min-h-11 rounded-md border border-border px-3 py-1.5 font-body text-xs font-medium text-text-secondary transition-colors duration-fast hover:border-border-strong hover:text-text-primary"
              >
                Try again
              </button>
            </div>
          )}

          {listQuery.isSuccess && notifications.length === 0 && (
            <div className="px-6 py-9 text-center">
              <span
                aria-hidden="true"
                className="mx-auto flex h-10 w-10 items-center justify-center rounded-full bg-moss-100 text-moss-700"
              >
                <Bell className="h-5 w-5" strokeWidth={1.7} />
              </span>
              <p className="mt-3 font-body text-sm font-medium text-text-primary">
                No notifications yet
              </p>
              <p className="mt-1 font-body text-xs text-text-secondary">
                Agent activity and mentions will appear here.
              </p>
            </div>
          )}

          {listQuery.isSuccess && notifications.length > 0 && (
            <ul
              aria-label="Notification history"
              className="max-h-96 divide-y divide-border overflow-y-auto"
            >
              {notifications.map((notification) => (
                <NotificationRow
                  key={notification.id}
                  notification={notification}
                  onSelect={handleNotificationClick}
                />
              ))}
            </ul>
          )}
        </section>
      )}
    </div>
  );
}
