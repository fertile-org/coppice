import type { ToolPart } from '../sync/types';
import { formatOutput, outputText, str } from './tool-utils';
import { ToolOutput, ToolShell } from './ToolShell';

export function Skill({ part }: { part: ToolPart }) {
  const name = str(part.state.input?.name);
  const output = outputText(part);
  const title = name ? `Skill "${name}"` : 'Loading skill...';

  return (
    <ToolShell tool="skill" status={part.state.status} title={title}>
      {output ? <ToolOutput text={formatOutput(output)} /> : null}
    </ToolShell>
  );
}
