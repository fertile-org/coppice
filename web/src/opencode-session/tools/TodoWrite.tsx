import type { ToolPart } from '../sync/types';
import { sessionTheme } from '../theme/session-theme';
import { str } from './tool-utils';
import { ToolShell } from './ToolShell';

interface ParsedTodo {
  status: string;
  content: string;
}

function parseTodos(value: unknown): ParsedTodo[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item) => {
    if (typeof item !== 'object' || item === null) return [];
    const record = item as Record<string, unknown>;
    const status = str(record.status);
    const content = str(record.content);
    return status && content ? [{ status, content }] : [];
  });
}

function statusIcon(status: string): string {
  if (status === 'completed') return '✓';
  if (status === 'cancelled') return '✗';
  if (status === 'in_progress') return '→';
  return '○';
}

export function TodoWrite({ part }: { part: ToolPart }) {
  const todos =
    parseTodos(part.state.metadata?.todos).length > 0
      ? parseTodos(part.state.metadata?.todos)
      : parseTodos(part.state.input?.todos);
  const title = todos.length > 0 ? 'Todos' : 'Updating todos...';

  return (
    <ToolShell tool="todowrite" status={part.state.status} title={title}>
      <ul className="flex flex-col gap-1">
        {todos.map((todo, index) => (
          <li key={index} className={`flex gap-2 text-xs ${sessionTheme.text}`}>
            <span className={sessionTheme.textMuted}>{statusIcon(todo.status)}</span>
            <span>{todo.content}</span>
          </li>
        ))}
      </ul>
    </ToolShell>
  );
}
