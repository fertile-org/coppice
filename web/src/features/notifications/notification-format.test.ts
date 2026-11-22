import { describe, expect, it } from 'vitest';
import {
  formatNotificationTimestamp,
  notificationBellLabel,
} from './notification-format';

describe('notification format helpers', () => {
  const now = new Date('2026-08-03T12:00:00.000Z');

  it('formats timestamps and bell labels', () => {
    expect(formatNotificationTimestamp('2026-08-03T11:55:00.000Z', now, 'en')).toContain('ago');
    expect(formatNotificationTimestamp('not-a-date', now, 'en')).toBe('Unknown time');
    expect(notificationBellLabel(4)).toBe('Notifications, 4 unread');
    expect(notificationBellLabel(0)).toBe('Notifications, no unread notifications');
  });
});
