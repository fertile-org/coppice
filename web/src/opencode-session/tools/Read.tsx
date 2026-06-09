import type { ToolPart } from '../sync/types';
import { formatOutput, outputText, pathFromInput } from './tool-utils';
import { ToolOutput, ToolShell } from './ToolShell';

export function Read({ part }: { part: ToolPart }) {
  const filePath = pathFromInput(part.state.input);
  const output = outputText(part);
  const title = filePath ? `Read ${filePath}` : 'Reading file...';

  return (
    <ToolShell tool="read" status={part.state.status} title={title}>
      {output ? <ToolOutput text={formatOutput(output)} /> : null}
    </ToolShell>
  );
}
