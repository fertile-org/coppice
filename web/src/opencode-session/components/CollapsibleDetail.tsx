import { type ReactNode, useEffect, useRef, useState } from 'react';
import { sessionTheme } from '../theme/session-theme';

export function excerptPreview(text: string, maxLen = 100): string {
  const oneLine = text.replace(/\s+/g, ' ').trim();
  if (oneLine.length <= maxLen) return oneLine;
  return `${oneLine.slice(0, maxLen)}…`;
}

interface CollapsibleDetailProps {
  label: string;
  preview?: string;
  badge?: ReactNode;
  streaming?: boolean;
  defaultOpen?: boolean;
  className?: string;
  children: ReactNode;
}

export function CollapsibleDetail({
  label,
  preview,
  badge,
  streaming = false,
  defaultOpen = false,
  className = '',
  children,
}: CollapsibleDetailProps) {
  const [open, setOpen] = useState(defaultOpen || streaming);
  const wasStreaming = useRef(streaming);

  useEffect(() => {
    if (streaming) {
      setOpen(true);
      return;
    }
    if (wasStreaming.current) {
      setOpen(false);
    }
    wasStreaming.current = streaming;
  }, [streaming]);

  return (
    <div className={className}>
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        className={`w-full text-left ${sessionTheme.fontMono}`}
        aria-expanded={open}
      >
        <span className={sessionTheme.thinking}>{label}</span>
        {badge ? <span className="ml-2 inline-flex align-middle">{badge}</span> : null}
        {!open && preview ? (
          <span className={sessionTheme.thinkingDetail}>: {preview}</span>
        ) : null}
      </button>
      {open ? (
        <div
          className={
            streaming
              ? 'mt-2 max-h-48 overflow-y-auto'
              : 'mt-2'
          }
        >
          {children}
        </div>
      ) : null}
    </div>
  );
}
