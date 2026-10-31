/** Verbatim product copy for connector status, hints, and the turn-off confirm. */

export const DOCS_URL = 'https://getcoppice.app/docs';
export const INSTALL_GUIDE_URL = `${DOCS_URL}/providers`;

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
export const NO_FREE_SIGN_IN_CHECK = ['kilo-code', 'copilot', 'qwen-code', 'kimi'] as const;

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

type ChatReadOnlyStatus = 'passed' | 'untested' | 'unavailable';

/** Per-CLI chat status. Unknown ids fail closed as unavailable. */
const CHAT_READ_ONLY: Record<string, { name: string; status: ChatReadOnlyStatus }> = {
  'claude-code': { name: 'Claude Code', status: 'untested' },
  cursor: { name: 'Cursor', status: 'untested' },
  codex: { name: 'Codex', status: 'passed' },
  'kilo-code': { name: 'Kilo Code', status: 'untested' },
  opencode: { name: 'OpenCode', status: 'passed' },
  copilot: { name: 'GitHub Copilot', status: 'passed' },
  'qwen-code': { name: 'Qwen Code', status: 'passed' },
  kimi: { name: 'Kimi Code', status: 'passed' },
  grok: { name: 'Grok', status: 'passed' },
  mock: { name: 'Mock', status: 'passed' },
};

function chatConnector(
  connectorId: string | null | undefined,
): { name: string; status: ChatReadOnlyStatus } | null {
  if (!connectorId) return null;
  return CHAT_READ_ONLY[connectorId] ?? { name: connectorId, status: 'unavailable' };
}

/** Warning when chat is on but the CLI has not been shown to stay read-only. */
export function chatIsolationBanner(connectorId: string | null | undefined): string | null {
  const entry = chatConnector(connectorId);
  if (!entry || entry.status !== 'untested') return null;
  return `Coppice hasn't confirmed that ${entry.name} stays read-only. The agent can still run commands on your computer.`;
}

/** Why chat is off for this connector. One sentence per CLI. */
export function chatUnavailableMessage(connectorId: string | null | undefined): string | null {
  const entry = chatConnector(connectorId);
  if (!entry || entry.status !== 'unavailable') return null;
  return `Chat isn't available for ${entry.name} yet, because Coppice can't keep it read-only.`;
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
