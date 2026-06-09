import { ReasoningPart } from '../parts/ReasoningPart';
import { TextPart } from '../parts/TextPart';
import { ToolPart } from '../parts/ToolPart';
import type { Message, Part } from '../sync/types';
import { sessionTheme } from '../theme/session-theme';

function formatDuration(ms: number): string {
  const secs = Math.round(ms / 1000);
  if (secs < 60) return `${secs}s`;
  return `${Math.floor(secs / 60)}m ${secs % 60}s`;
}

function partRenderer(part: Part) {
  switch (part.type) {
    case 'text':
      return <TextPart key={part.id} part={part} />;
    case 'reasoning':
      return <ReasoningPart key={part.id} part={part} />;
    case 'tool':
      return <ToolPart key={part.id} part={part} />;
    default:
      return null;
  }
}

export function AssistantMessage({
  message,
  parts,
}: {
  message: Message;
  parts: Part[];
}) {
  const isComplete =
    message.time?.completed != null || message.finish != null;
  const duration =
    message.time?.created != null && message.time?.completed != null
      ? formatDuration(message.time.completed - message.time.created)
      : null;
  const footerParts = [message.mode, message.modelID, duration].filter(
    Boolean,
  );

  return (
    <div className="flex flex-col">
      {parts.map(partRenderer)}
      {message.error?.data?.message && (
        <p className={`ml-3 mt-1 font-body text-xs ${sessionTheme.error}`}>
          {message.error.data.message}
        </p>
      )}
      {isComplete && footerParts.length > 0 && (
        <p className={`ml-3 mt-1 font-body text-xs ${sessionTheme.textMuted}`}>
          {footerParts.join(' · ')}
        </p>
      )}
    </div>
  );
}
