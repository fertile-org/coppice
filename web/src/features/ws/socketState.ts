/**
 * Whether the shared event socket is open. Kept apart from useEventSocket so
 * queries can check it without importing the socket's dependencies.
 */
let open = false;

export function setEventSocketOpen(value: boolean) {
  open = value;
}

/** True while live data arrives as events, so polling can stop. */
export function isEventSocketOpen(): boolean {
  return open;
}
