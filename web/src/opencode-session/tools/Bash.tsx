import { PlainOutput } from '../components/PlainOutput';
import type { ToolPart } from '../sync/types';
import { formatOutput, outputText, str } from './tool-utils';
import { ToolOutput, ToolShell } from './ToolShell';

export function Bash({ part }: { part: ToolPart }) {
  const command = str(part.state.input?.command);
  const output = outputText(part);

  return (
    <ToolShell tool="bash" status={part.state.status} title={command || '$'}>
      {output ? (
        <ToolOutput>
          <PlainOutput text={formatOutput(output)} language="bash" />
        </ToolOutput>
      ) : null}
    </ToolShell>
  );
}
