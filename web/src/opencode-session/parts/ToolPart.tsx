import type { ComponentType } from 'react';
import type { ToolPart as ToolPartType } from '../sync/types';
import { sessionTheme } from '../theme/session-theme';
import { ApplyPatch } from '../tools/ApplyPatch';
import { Bash } from '../tools/Bash';
import { Edit } from '../tools/Edit';
import { Glob } from '../tools/Glob';
import { Grep } from '../tools/Grep';
import { List } from '../tools/List';
import { Question } from '../tools/Question';
import { Read } from '../tools/Read';
import { Skill } from '../tools/Skill';
import { Task } from '../tools/Task';
import { TodoWrite } from '../tools/TodoWrite';
import { WebFetch } from '../tools/WebFetch';
import { Write } from '../tools/Write';

const TOOL_MAP: Record<string, ComponentType<{ part: ToolPartType }>> = {
  bash: Bash,
  read: Read,
  write: Write,
  edit: Edit,
  grep: Grep,
  glob: Glob,
  ls: List,
  webfetch: WebFetch,
  task: Task,
  skill: Skill,
  question: Question,
  todowrite: TodoWrite,
  apply_patch: ApplyPatch,
};

function UnknownTool({ part }: { part: ToolPartType }) {
  const input = JSON.stringify(part.state.input ?? {}, null, 0).slice(0, 120);
  return (
    <div className={`${sessionTheme.fontMono} ${sessionTheme.toolComplete}`}>
      → {part.tool} {input || '(no input)'}
    </div>
  );
}

export function ToolPart({ part }: { part: ToolPartType }) {
  const Component = TOOL_MAP[part.tool] ?? UnknownTool;
  return <Component part={part} />;
}
