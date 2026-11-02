import type { ConfirmFn } from '../../components/ui/confirm-dialog';
import type { Plugin } from '../../lib/schemas/plugin';

const STDIO_RISK =
  "Local MCP servers run on your computer with your computer's privileges.";

/** Names keys only; the values never reach the browser. */
export function envRisk(keys: string[]): string {
  const list = keys.join(', ');
  return keys.length === 1
    ? `Setting ${list} is not set here, so your computer's environment value for it will be sent to the plugin.`
    : `Settings ${list} are not set here, so your computer's environment values for them will be sent to the plugin.`;
}

/** Starting a disabled plugin's servers (enable or Test) needs the admin's consent. */
export async function confirmStart(
  plugin: Plugin,
  confirm: ConfirmFn,
): Promise<boolean> {
  if (plugin.enabled) return true;
  const envKeys = plugin.settings.filter((s) => s.source === 'env').map((s) => s.key);
  const risks = [
    plugin.mcpServers.some((server) => server.kind === 'stdio') ? STDIO_RISK : null,
    envKeys.length > 0 ? envRisk(envKeys) : null,
  ].filter((risk): risk is string => risk !== null);
  return (
    risks.length === 0 ||
    confirm({ title: `${risks.join(' ')} Enable anyway?`, danger: true })
  );
}
