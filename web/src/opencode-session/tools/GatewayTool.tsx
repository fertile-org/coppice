import type { ToolPart } from '../sync/types';
import { sessionTheme } from '../theme/session-theme';
import { gatewayToolTitle, type GatewayTool as GatewayToolName } from './gateway-tool';

export function GatewayTool({ part, name }: { part: ToolPart; name: GatewayToolName }) {
  const input = JSON.stringify(part.state.input ?? {}, null, 0).slice(0, 120);
  return (
    <div className={`${sessionTheme.fontMono} ${sessionTheme.toolComplete}`}>
      → {gatewayToolTitle(name)} {input || '(no input)'}
    </div>
  );
}
