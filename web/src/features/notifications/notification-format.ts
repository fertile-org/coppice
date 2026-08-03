export function notificationBellLabel(unreadCount: number): string {
  if (unreadCount <= 0) {
    return 'Notifications, no unread notifications';
  }
  return `Notifications, ${unreadCount} unread`;
}

export function formatNotificationTimestamp(
  createdAt: string,
  now = new Date(),
  locale?: string,
): string {
  const timestamp = new Date(createdAt);
  if (Number.isNaN(timestamp.getTime())) {
    return 'Unknown time';
  }

  const differenceMs = timestamp.getTime() - now.getTime();
  const absoluteMs = Math.abs(differenceMs);
  if (absoluteMs < 60_000) {
    return 'Just now';
  }

  const formatter = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' });
  if (absoluteMs < 60 * 60_000) {
    return formatter.format(Math.round(differenceMs / 60_000), 'minute');
  }
  if (absoluteMs < 24 * 60 * 60_000) {
    return formatter.format(Math.round(differenceMs / (60 * 60_000)), 'hour');
  }
  if (absoluteMs < 7 * 24 * 60 * 60_000) {
    return formatter.format(
      Math.round(differenceMs / (24 * 60 * 60_000)),
      'day',
    );
  }

  return new Intl.DateTimeFormat(locale, {
    month: 'short',
    day: 'numeric',
    ...(timestamp.getFullYear() === now.getFullYear()
      ? {}
      : { year: 'numeric' as const }),
  }).format(timestamp);
}

export function formatNotificationDateTime(
  createdAt: string,
  locale?: string,
): string | undefined {
  const timestamp = new Date(createdAt);
  if (Number.isNaN(timestamp.getTime())) {
    return undefined;
  }
  return new Intl.DateTimeFormat(locale, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(timestamp);
}
