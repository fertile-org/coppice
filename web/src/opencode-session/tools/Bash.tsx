import type { ToolPart } from '../sync/types';
import { formatOutput, outputText, str } from './tool-utils';
import { ToolOutput, ToolShell } from './ToolShell';

export function Bash({ part }: { part: ToolPart }) {
  const command = str(part.state.input?.command);
  const output = outputText(part);

  return (
    <ToolShell tool="bash" status={part.state.status} title={command || '$'}>
      {output ? <ToolOutput text={formatOutput(output)} /> : null}
    </ToolShell>
  );
}
