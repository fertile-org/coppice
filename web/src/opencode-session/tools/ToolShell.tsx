import type { ReactNode } from 'react';
import { sessionTheme } from '../theme/session-theme';

function statusIcon(status: string): string {
  if (status === 'error') return '✗';
  if (status === 'completed') return '✓';
  return '→';
}

function statusClass(status: string): string {
  if (status === 'error') return sessionTheme.error;
  if (status === 'completed') return sessionTheme.textMuted;
  return sessionTheme.accent;
}

interface ToolShellProps {
  tool: string;
  status: string;
  title: string;
  children?: ReactNode;
}

export function ToolShell({ tool, status, title, children }: ToolShellProps) {
  const icon = statusIcon(status);
  const colorClass = statusClass(status);
  const showOutput = status === 'completed' && children;

  return (
    <div className={`ml-2 mt-1 border-l-2 py-1 pl-2 ${sessionTheme.border}`}>
      <div className={`flex items-start gap-1 font-mono text-xs ${colorClass}`}>
        <span className="w-3 shrink-0">{icon}</span>
        <span className={`min-w-0 flex-1 break-all ${sessionTheme.text}`}>
          <span className={sessionTheme.textMuted}>{tool}: </span>
          {title || '(no input)'}
        </span>
      </div>
      {showOutput ? <div className="mt-1 pl-4">{children}</div> : null}
    </div>
  );
}

export function ToolOutput({ text }: { text: string }) {
  return (
    <pre className={`overflow-x-auto whitespace-pre-wrap text-xs ${sessionTheme.text}`}>{text}</pre>
  );
}
