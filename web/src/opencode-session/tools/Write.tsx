import { FileContentView } from '../components/FileContentView';
import { PlainOutput } from '../components/PlainOutput';
import type { ToolPart } from '../sync/types';
import { formatOutput, outputText, pathFromInput, str } from './tool-utils';
import { parseFileToolOutput } from './parse-opencode-output';
import { ToolOutput, ToolShell } from './ToolShell';

export function Write({ part }: { part: ToolPart }) {
  const filePath = pathFromInput(part.state.input);
  const inputContent = str(part.state.input?.content);
  const raw = outputText(part) ?? (inputContent || undefined);
  const parsed = raw ? parseFileToolOutput(raw) : null;
  const title = filePath ? `Write ${filePath}` : 'Preparing write...';

  return (
    <ToolShell tool="write" status={part.state.status} title={title}>
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
