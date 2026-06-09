import { FileContentView } from '../components/FileContentView';
import { PlainOutput } from '../components/PlainOutput';
import type { ToolPart } from '../sync/types';
import { outputText, pathFromInput } from './tool-utils';
import { parseFileToolOutput } from './parse-opencode-output';
import { ToolOutput, ToolShell } from './ToolShell';

export function Read({ part }: { part: ToolPart }) {
  const filePath = pathFromInput(part.state.input);
  const raw = outputText(part);
  const parsed = raw ? parseFileToolOutput(raw) : null;
  const title = filePath ? `Read ${filePath}` : parsed?.path ? `Read ${parsed.path}` : 'Read';

  return (
    <ToolShell tool="read" status={part.state.status} title={title}>
      {parsed ? (
        <ToolOutput>
          <FileContentView path={parsed.path ?? filePath} content={parsed.content} />
        </ToolOutput>
      ) : raw ? (
        <ToolOutput>
          <PlainOutput text={raw} />
        </ToolOutput>
      ) : null}
    </ToolShell>
  );
}
