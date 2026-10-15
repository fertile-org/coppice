/** Verbatim product copy for connector status, hints, and the turn-off confirm. */

export const INSTALL_GUIDE_URL = 'https://getcoppice.vercel.app/docs/providers';

export const NO_READY_CONNECTOR_HINT =
  "No agent CLI is ready yet. Pick the one you plan to use, and save now. It will work once it's installed and signed in.";

export type ConnectorReadiness = 'ready' | 'found_not_signed_in' | 'not_on_path' | 'found';

/** User-facing status. A missing value makes no sign-in claim. */
export function readinessLabel(readiness: ConnectorReadiness | null | undefined): string | null {
  switch (readiness) {
    case 'ready':
      return 'Ready';
    case 'found_not_signed_in':
      return 'Found, not signed in';
    case 'not_on_path':
      return 'Not on your PATH';
    case 'found':
      return 'Installed';
    default:
      return null;
  }
}

/** Connectors whose sign-in cannot be checked without a plan-using request. */
export const NO_FREE_SIGN_IN_CHECK = ['kilo-code'] as const;

export const SIGN_IN_UNCHECKED_HINT =
  "Coppice can't check this sign-in without using your plan. A run will show an error if you're signed out.";

export function signInUncheckedHint(
  connectorId: string | null | undefined,
  readiness: ConnectorReadiness | null | undefined,
): string | null {
  if (readiness !== 'found' || !connectorId) return null;
  return (NO_FREE_SIGN_IN_CHECK as readonly string[]).includes(connectorId)
    ? SIGN_IN_UNCHECKED_HINT
    : null;
}

/** Chat turns are read-only. These connectors cannot enforce that, so Chat stays off. */
export const KILO_CHAT_UNAVAILABLE =
  "Chat isn't available for Kilo Code yet, because Chat runs read-only and Kilo Code can't.";

export const CODEX_CHAT_UNAVAILABLE =
  "Chat isn't available for Codex yet, because Chat runs read-only and Codex can't.";

export const CURSOR_CHAT_UNAVAILABLE =
  "Chat isn't available for Cursor yet, because Chat runs read-only and Cursor can't.";

const CHAT_UNAVAILABLE_BY_CONNECTOR: Record<string, string> = {
  'kilo-code': KILO_CHAT_UNAVAILABLE,
  codex: CODEX_CHAT_UNAVAILABLE,
  cursor: CURSOR_CHAT_UNAVAILABLE,
};

export function chatUnavailableMessage(connectorId: string | null | undefined): string | null {
  if (!connectorId) return null;
  return CHAT_UNAVAILABLE_BY_CONNECTOR[connectorId] ?? null;
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

/** First model option. `{connector}` is the connector's display name. */
export const CONNECTOR_DEFAULT_MODEL_LABEL = "{connector}'s default";

export function connectorDefaultModelLabel(connectorName: string): string {
  return CONNECTOR_DEFAULT_MODEL_LABEL.replaceAll('{connector}', connectorName);
}

export function turnOffConfirm(name: string, count: number): string {
  if (count === 1) {
    return `Turn off ${name}? 1 agent uses it and can't run tickets until you turn it back on or switch it to another connector.`;
  }
  return `Turn off ${name}? ${count} agents use it and can't run tickets until you turn it back on or switch them to another connector.`;
}
