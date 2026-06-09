import type { ToolPart } from '../sync/types';
import { formatOutput, num, outputText, pathFromInput, str } from './tool-utils';
import { ToolOutput, ToolShell } from './ToolShell';

export function Grep({ part }: { part: ToolPart }) {
  const pattern = str(part.state.input?.pattern);
  const searchPath = pathFromInput(part.state.input);
  const matches = num(part.state.metadata?.matches);
  const output = outputText(part);

  let title = pattern ? `Grep "${pattern}"` : 'Searching content...';
  if (searchPath) title += ` in ${searchPath}`;
  if (matches !== undefined) {
    title += ` (${matches} ${matches === 1 ? 'match' : 'matches'})`;
  }

  return (
    <ToolShell tool="grep" status={part.state.status} title={title}>
      {output ? <ToolOutput text={formatOutput(output)} /> : null}
    </ToolShell>
  );
}
