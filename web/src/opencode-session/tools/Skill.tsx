import { PlainOutput } from '../components/PlainOutput';
import type { ToolPart } from '../sync/types';
import { formatOutput, outputText, str } from './tool-utils';
import { ToolOutput, ToolShell } from './ToolShell';

export function Skill({ part }: { part: ToolPart }) {
  const name = str(part.state.input?.name);
  const output = outputText(part);
  const title = name ? `Skill "${name}"` : 'Loading skill...';

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
