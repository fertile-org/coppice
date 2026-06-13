import { type ReactNode, useEffect, useRef, useState } from 'react';
import { OutputBlock } from '../components/OutputBlock';
import { sessionTheme } from '../theme/session-theme';

function isTerminalStatus(status: string): boolean {
  return status === 'completed' || status === 'error';
}

function lineClass(status: string): string {
  if (status === 'error') return sessionTheme.error;
  if (status === 'completed') return sessionTheme.toolComplete;
  return sessionTheme.toolActive;
}

interface ToolShellProps {
  status: string;
  title: string;
  /** `action` → prefix, `shell` $ prefix (bash). */
  variant?: 'action' | 'shell';
  children?: ReactNode;
}

export function ToolShell({
  status,
  title,
  variant = 'action',
  children,
}: ToolShellProps) {
  const colorClass = lineClass(status);
  const hasOutput = Boolean(children);
  const showOutput = isTerminalStatus(status) && hasOutput;
  const streaming = hasOutput && !isTerminalStatus(status);

  const [outputOpen, setOutputOpen] = useState(false);
  const wasStreaming = useRef(streaming);

  useEffect(() => {
    if (streaming) {
      setOutputOpen(true);
      return;
    }
    if (wasStreaming.current) {
      setOutputOpen(false);
    }
    wasStreaming.current = streaming;
  }, [streaming]);

  const toggleOutput = () => {
    if (showOutput) setOutputOpen((value) => !value);
  };

  const prefix = variant === 'shell' ? '$ ' : '→ ';
  const lineText = title || (variant === 'shell' ? '' : '(no input)');
  const active = !isTerminalStatus(status);

  return (
    <div>
      <div
        className={`${sessionTheme.fontMono} ${colorClass} ${
          showOutput ? 'cursor-pointer select-none' : ''
        }`}
        onClick={showOutput ? toggleOutput : undefined}
        onKeyDown={
          showOutput
            ? (event) => {
                if (event.key === 'Enter' || event.key === ' ') {
                  event.preventDefault();
                  toggleOutput();
                }
              }
            : undefined
        }
        role={showOutput ? 'button' : undefined}
        tabIndex={showOutput ? 0 : undefined}
        aria-expanded={showOutput ? outputOpen : undefined}
      >
        {prefix}
        {lineText}
        {active ? (
          <span className="ml-2 inline-flex items-center gap-1 align-middle" aria-hidden>
            {[0, 150, 300].map((delay) => (
              <span
                key={delay}
                className="inline-block h-1 w-1 animate-bounce rounded-full bg-[var(--oc-tool-active)]"
                style={{ animationDelay: `${delay}ms` }}
              />
            ))}
          </span>
        ) : null}
      </div>
      {showOutput && outputOpen ? (
        <OutputBlock>{children}</OutputBlock>
      ) : null}
      {streaming ? (
        <div className="mt-2 max-h-48 overflow-y-auto">
          <OutputBlock>{children}</OutputBlock>
        </div>
      ) : null}
    </div>
  );
}

export function ToolOutput({ children }: { children: ReactNode }) {
  return <>{children}</>;
}
