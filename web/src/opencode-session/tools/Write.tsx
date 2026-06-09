import type { ToolPart } from '../sync/types';
import { formatOutput, outputText, pathFromInput, str } from './tool-utils';
import { ToolOutput, ToolShell } from './ToolShell';

export function Write({ part }: { part: ToolPart }) {
  const filePath = pathFromInput(part.state.input);
  const content = str(part.state.input?.content);
  const output = outputText(part) ?? (content || undefined);
  const title = filePath ? `Write ${filePath}` : 'Preparing write...';

  return (
    <ToolShell tool="write" status={part.state.status} title={title}>
      {output ? <ToolOutput text={formatOutput(output)} /> : null}
    </ToolShell>
  );
}
