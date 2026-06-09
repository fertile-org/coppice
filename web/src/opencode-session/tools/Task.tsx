import type { ToolPart } from '../sync/types';
import { formatOutput, outputText, str } from './tool-utils';
import { ToolOutput, ToolShell } from './ToolShell';

export function Task({ part }: { part: ToolPart }) {
  const description = str(part.state.input?.description);
  const prompt = str(part.state.input?.prompt);
  const output = outputText(part);

  let title = description || 'Delegating...';
  if (prompt && !title.includes(prompt.slice(0, 40))) {
    title = `${title} — ${prompt.length > 80 ? `${prompt.slice(0, 80)}…` : prompt}`;
  }

  return (
    <ToolShell tool="task" status={part.state.status} title={title}>
      {output ? <ToolOutput text={formatOutput(output)} /> : null}
    </ToolShell>
  );
}
