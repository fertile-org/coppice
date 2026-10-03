/** Gateway MCP server name; matches the server's `protocol::SERVER_NAME`. */
export const GATEWAY_SERVER = 'coppice';

const GATEWAY_PREFIX = `${GATEWAY_SERVER}_`;

export interface GatewayTool {
  plugin: string | null;
  tool: string;
}

/**
 * OpenCode names MCP tools `<server>_<tool>`; plugin tools are exposed as
 * `<plugin>__<tool>` and core tool names never contain `__`.
 */
export function parseGatewayTool(name: string): GatewayTool | null {
  if (!name.startsWith(GATEWAY_PREFIX)) return null;
  const exposed = name.slice(GATEWAY_PREFIX.length);
  if (!exposed) return null;
  const split = exposed.indexOf('__');
  if (split > 0 && split + 2 < exposed.length) {
    return { plugin: exposed.slice(0, split), tool: exposed.slice(split + 2) };
  }
  return { plugin: null, tool: exposed };
}

export function gatewayToolTitle(tool: GatewayTool): string {
  return `${tool.plugin ?? GATEWAY_SERVER} · ${tool.tool}`;
}
