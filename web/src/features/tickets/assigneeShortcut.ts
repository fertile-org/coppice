const OPEN_ASSIGNEE = 'coppice-open-assignee';

let pendingTicketId: string | null = null;

export function openAssigneeCombobox() {
  const button = document.getElementById('ticket-assignee');
  if (!(button instanceof HTMLButtonElement) || button.disabled) return;
  if (button.getAttribute('aria-expanded') === 'true') return;
  button.click();
}

export function requestAssigneeOpen(ticketId: string) {
  pendingTicketId = ticketId;
  window.dispatchEvent(new Event(OPEN_ASSIGNEE));
}

export function subscribeAssigneeOpen(ticketId: string, open: () => void): () => void {
  function handle() {
    if (pendingTicketId !== ticketId) return;
    pendingTicketId = null;
    open();
  }
  handle();
  window.addEventListener(OPEN_ASSIGNEE, handle);
  return () => window.removeEventListener(OPEN_ASSIGNEE, handle);
}
