import { ApiError, parseApiErrorMessage } from '../../lib/api';

const MODEL_VALUE_SEP = '\u001f';

/** `{Connector}'s models` failure. `{connector}` is the display name. */
export function modelsUnavailableMessage(connectorName: string): string {
  return `Couldn't get ${connectorName}'s models. Using ${connectorName}'s default for now.`;
}

const TURNED_OFF_LOG =
  /^Can't list (.+)'s models because it's turned off in Connectors\.$/;

/** Display name from the server's turned-off log line, or null. */
export function turnedOffConnectorName(
  message: string | null | undefined,
): string | null {
  const match = message?.match(TURNED_OFF_LOG);
  return match?.[1] ?? null;
}

export const CURSOR_MODELS_SIGN_IN =
  "Cursor's CLI isn't signed in for Coppice. Run `agent login` in a terminal, then click Refresh models.";

/** Small text under the failure message. Cursor's signed-out CLI replaces the stderr line. */
export function modelsFailureExtra(
  connectorId: string,
  detail: string | null | undefined,
): string | null {
  const line = detail?.trim() ?? '';
  if (!line) return null;
  if (connectorId === 'cursor' && line.includes('Authentication required')) {
    return CURSOR_MODELS_SIGN_IN;
  }
  return line;
}

export function connectorModelsPath(
  connectorId: string,
  providerId: string,
  refresh: boolean,
): string {
  const path = `/api/connectors/${encodeURIComponent(connectorId)}/model-providers/${encodeURIComponent(providerId)}/models`;
  return refresh ? `${path}?refresh=true` : path;
}

export function encodeModelValue(provider: string, model: string): string {
  return `${provider}${MODEL_VALUE_SEP}${model}`;
}

export function decodeModelValue(
  value: string,
): { provider: string; model: string } | null {
  const sep = value.indexOf(MODEL_VALUE_SEP);
  if (sep < 0) return null;
  return { provider: value.slice(0, sep), model: value.slice(sep + 1) };
}

export class ModelsRequestError extends Error {
  detail: string | null;

  constructor(message: string, detail: string | null) {
    super(message);
    this.name = 'ModelsRequestError';
    this.detail = detail;
  }
}

export function modelsRequestError(err: unknown): ModelsRequestError {
  if (err instanceof ModelsRequestError) return err;
  if (err instanceof ApiError) {
    let detail: string | null = null;
    try {
      const parsed = JSON.parse(err.body) as { detail?: unknown };
      if (typeof parsed.detail === 'string' && parsed.detail.trim()) {
        detail = parsed.detail.trim();
      }
    } catch {
      detail = null;
    }
    return new ModelsRequestError(parseApiErrorMessage(err, err.message), detail);
  }
  return new ModelsRequestError(err instanceof Error ? err.message : 'Request failed', null);
}

/** Saved model plus provider, or '' for the connector default. */
export function modelSelectionValue(
  modelProvider: string,
  model: string,
): string {
  if (!model.trim()) return '';
  return encodeModelValue(modelProvider.trim(), model.trim());
}
