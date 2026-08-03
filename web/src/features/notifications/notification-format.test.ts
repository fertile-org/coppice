import { describe, expect, it } from 'vitest';
import {
  formatNotificationTimestamp,
  notificationBellLabel,
} from './notification-format';

describe('formatNotificationTimestamp', () => {
  const now = new Date('2026-08-03T12:00:00.000Z');

  it('uses concise relative times for recent notifications', () => {
    expect(
      formatNotificationTimestamp('2026-08-03T11:55:00.000Z', now, 'en'),
    ).toBe('5 minutes ago');
    expect(
      formatNotificationTimestamp('2026-08-03T09:00:00.000Z', now, 'en'),
    ).toBe('3 hours ago');
  });

  it('falls back to a formatted date for older notifications', () => {
    expect(
      formatNotificationTimestamp('2026-07-12T09:00:00.000Z', now, 'en'),
    ).toBe('Jul 12');
  });

  it('returns a safe label for invalid timestamps', () => {
    expect(formatNotificationTimestamp('not-a-date', now, 'en')).toBe(
      'Unknown time',
    );
  });
});

describe('notificationBellLabel', () => {
  it('includes the unread count in the accessible label', () => {
    expect(notificationBellLabel(4)).toBe('Notifications, 4 unread');
    expect(notificationBellLabel(0)).toBe(
      'Notifications, no unread notifications',
    );
  });
});
