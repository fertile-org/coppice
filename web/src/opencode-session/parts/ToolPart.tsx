import type { ComponentType } from 'react';
import type { ToolPart as ToolPartType } from '../sync/types';
import { sessionTheme } from '../theme/session-theme';

const TOOL_MAP: Record<string, ComponentType<{ part: ToolPartType }>> = {};

function UnknownTool({ part }: { part: ToolPartType }) {
  const input = JSON.stringify(part.state.input ?? {}, null, 0).slice(0, 120);
  return (
    <div className={`ml-2 mt-1 font-mono text-xs ${sessionTheme.textMuted}`}>
      → {part.tool}: {input || '(no input)'}
    </div>
  );
}

export function ToolPart({ part }: { part: ToolPartType }) {
  const Component = TOOL_MAP[part.tool] ?? UnknownTool;
  return <Component part={part} />;
}
