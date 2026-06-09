import { PlainOutput } from '../components/PlainOutput';
import type { ToolPart } from '../sync/types';
import { formatOutput, pathFromInput, str } from './tool-utils';
import { ToolOutput, ToolShell } from './ToolShell';

export function ApplyPatch({ part }: { part: ToolPart }) {
  const patch = str(part.state.input?.patch);
  const file = pathFromInput(part.state.input);
  const files = part.state.metadata?.files;

  let title = 'Patch';
  if (file) {
    title = `Patch ${file}`;
  } else if (patch) {
    title = 'Applying patch...';
  }

  let output: string | undefined;
  if (Array.isArray(files) && files.length > 0) {
    output = files
      .flatMap((item) => {
        if (typeof item !== 'object' || item === null) return [];
        const record = item as Record<string, unknown>;
        const relativePath = str(record.relativePath);
        const filePatch = str(record.patch);
        const type = str(record.type);
        if (!relativePath) return [];
        if (type === 'delete') return [`Deleted ${relativePath}`];
        if (filePatch) return [`${relativePath}\n${filePatch}`];
        return [relativePath];
      })
      .join('\n\n');
  } else if (patch) {
    output = patch;
  }

  return (
    <ToolShell tool="apply_patch" status={part.state.status} title={title}>
      {output ? (
        <ToolOutput>
          <PlainOutput text={formatOutput(output)} />
        </ToolOutput>
      ) : null}
    </ToolShell>
  );
}
