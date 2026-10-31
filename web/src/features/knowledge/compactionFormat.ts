export function formatInterval(seconds: number): string {
  if (seconds < 60) return `${seconds} s`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.round((minutes / 60) * 10) / 10;
  return hours === 1 ? '1 hour' : `${hours} hours`;
}

export function pluralTickets(count: number, noun = 'ticket'): string {
  return `${count} ${noun}${count === 1 ? '' : 's'}`;
}

/** "3 days", "5 hours", "20 minutes": how long ago an ISO timestamp was. */
export function formatAge(iso: string, now = Date.now()): string {
  const minutes = Math.max(0, Math.floor((now - new Date(iso).getTime()) / 60_000));
  const unit = (count: number, noun: string) => `${count} ${noun}${count === 1 ? '' : 's'}`;
  if (minutes < 60) return unit(minutes, 'minute');
  if (minutes < 24 * 60) return unit(Math.floor(minutes / 60), 'hour');
  return unit(Math.floor(minutes / (24 * 60)), 'day');
}
