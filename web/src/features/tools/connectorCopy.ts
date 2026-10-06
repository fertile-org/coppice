/** Verbatim product copy for connector status, hints, and the turn-off confirm. */

export const INSTALL_GUIDE_URL = 'https://getcoppice.vercel.app/docs/providers';

export const NO_READY_CONNECTOR_HINT =
  "No agent CLI is ready yet. Pick the one you plan to use, and save now. It will work once it's installed and signed in.";

export type ConnectorReadiness = 'ready' | 'found_not_signed_in' | 'not_on_path' | 'found';

/** User-facing status. `found` and a missing value make no sign-in claim. */
export function readinessLabel(readiness: ConnectorReadiness | null | undefined): string | null {
  switch (readiness) {
    case 'ready':
      return 'Ready';
    case 'found_not_signed_in':
      return 'Found, not signed in';
    case 'not_on_path':
      return 'Not on your PATH';
    default:
      return null;
  }
}

export function notOnPathHint(name: string): string {
  return `${name} isn't installed on this machine yet. Install it and sign in, then this agent can run tickets. You can still save now.`;
}

export function notSignedInHint(name: string): string {
  return `Sign in to ${name} in your terminal before this agent runs a ticket.`;
}

export function turnedOnToast(name: string): string {
  return `${name} turned on.`;
}

export function turnOffConfirm(name: string, count: number): string {
  if (count === 1) {
    return `Turn off ${name}? 1 agent uses it and can't run tickets until you turn it back on or switch it to another connector.`;
  }
  return `Turn off ${name}? ${count} agents use it and can't run tickets until you turn it back on or switch them to another connector.`;
}
