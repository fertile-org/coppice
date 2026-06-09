import type { ToolPart } from '../sync/types';
import { formatOutput, pathFromInput, str } from './tool-utils';
import { ToolOutput, ToolShell } from './ToolShell';

export function Edit({ part }: { part: ToolPart }) {
  const filePath = pathFromInput(part.state.input);
  const diff = str(part.state.metadata?.diff);
  const output = diff || undefined;
  const title = filePath ? `Edit ${filePath}` : 'Preparing edit...';

  return (
    <ToolShell tool="edit" status={part.state.status} title={title}>
      {output ? <ToolOutput text={formatOutput(output)} /> : null}
    </ToolShell>
  );
}
