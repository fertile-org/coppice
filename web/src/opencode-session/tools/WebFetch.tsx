import type { ToolPart } from '../sync/types';
import { formatOutput, outputText, str } from './tool-utils';
import { ToolOutput, ToolShell } from './ToolShell';

export function WebFetch({ part }: { part: ToolPart }) {
  const url = str(part.state.input?.url);
  const output = outputText(part);
  const title = url || 'Fetching from the web...';

  return (
    <ToolShell tool="webfetch" status={part.state.status} title={title}>
      {output ? <ToolOutput text={formatOutput(output)} /> : null}
    </ToolShell>
  );
}
