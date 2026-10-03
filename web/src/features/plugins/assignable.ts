import type { Plugin } from '../../lib/schemas/plugin';

/** Agents can only be given enabled plugins that loaded cleanly. */
export function isAssignable(plugin: Plugin): boolean {
  return plugin.enabled && plugin.status === 'ok';
}
