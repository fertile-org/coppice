import { PlainOutput } from '../components/PlainOutput';
import type { ToolPart } from '../sync/types';
import { formatOutput, num, outputText, pathFromInput, str } from './tool-utils';
import { ToolOutput, ToolShell } from './ToolShell';

export function Glob({ part }: { part: ToolPart }) {
  const pattern = str(part.state.input?.pattern);
  const searchPath = pathFromInput(part.state.input);
  const count = num(part.state.metadata?.count);
  const output = outputText(part);

  let title = pattern ? `Glob "${pattern}"` : 'Finding files...';
  if (searchPath) title += ` in ${searchPath}`;
  if (count !== undefined) {
    title += ` (${count} ${count === 1 ? 'match' : 'matches'})`;
  }

  return (
    <ToolShell status={part.state.status} title={title}>
      {output ? (
        <ToolOutput>
          <PlainOutput text={formatOutput(output)} />
        </ToolOutput>
      ) : null}
    </ToolShell>
  );
}
