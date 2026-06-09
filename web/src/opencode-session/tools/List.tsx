import { PlainOutput } from '../components/PlainOutput';
import type { ToolPart } from '../sync/types';
import { formatOutput, outputText, pathFromInput } from './tool-utils';
import { ToolOutput, ToolShell } from './ToolShell';

export function List({ part }: { part: ToolPart }) {
  const dirPath = pathFromInput(part.state.input) || '.';
  const output = outputText(part);
  const title = `List ${dirPath}`;

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
