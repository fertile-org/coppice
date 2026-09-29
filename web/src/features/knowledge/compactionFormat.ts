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
