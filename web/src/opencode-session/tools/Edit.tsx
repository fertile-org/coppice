import { FileContentView } from '../components/FileContentView';
import { PlainOutput } from '../components/PlainOutput';
import type { ToolPart } from '../sync/types';
import { formatOutput, outputText, pathFromInput } from './tool-utils';
import { parseFileToolOutput } from './parse-opencode-output';
import { ToolOutput, ToolShell } from './ToolShell';

export function Edit({ part }: { part: ToolPart }) {
  const filePath = pathFromInput(part.state.input);
  const raw = outputText(part);
  const parsed = raw ? parseFileToolOutput(raw) : null;
  const title = filePath ? `Edit ${filePath}` : 'Edit';

  return (
    <ToolShell status={part.state.status} title={title}>
      {parsed ? (
        <ToolOutput>
          <FileContentView path={parsed.path ?? filePath} content={parsed.content} />
        </ToolOutput>
      ) : raw ? (
        <ToolOutput>
          <PlainOutput text={formatOutput(raw)} />
        </ToolOutput>
      ) : null}
    </ToolShell>
  );
}
