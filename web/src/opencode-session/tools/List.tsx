import type { ToolPart } from '../sync/types';
import { formatOutput, outputText, pathFromInput } from './tool-utils';
import { ToolOutput, ToolShell } from './ToolShell';

export function List({ part }: { part: ToolPart }) {
  const dirPath = pathFromInput(part.state.input) || '.';
  const output = outputText(part);
  const title = `List ${dirPath}`;

  return (
    <ToolShell tool="ls" status={part.state.status} title={title}>
      {output ? <ToolOutput text={formatOutput(output)} /> : null}
    </ToolShell>
  );
}
