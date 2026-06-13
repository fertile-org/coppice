import { SessionSectionCard } from '../components/SessionSectionCard';
import { CompactionPart } from '../parts/CompactionPart';
import { ReasoningPart } from '../parts/ReasoningPart';
import { TextPart } from '../parts/TextPart';
import { ToolPart } from '../parts/ToolPart';
import type { Message, Part } from '../sync/types';
import { sessionTheme } from '../theme/session-theme';

function renderPart(
  part: Part,
  index: number,
  parts: Part[],
  messageComplete: boolean,
) {
  const reasoningStreaming =
    part.type === 'reasoning' &&
    !messageComplete &&
    index === parts.length - 1;

  switch (part.type) {
    case 'text':
      return <TextPart key={part.id} part={part} />;
    case 'reasoning':
      return (
        <ReasoningPart
          key={part.id}
          part={part}
          streaming={reasoningStreaming}
        />
      );
    case 'compaction':
      return <CompactionPart key={part.id} part={part} />;
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
  const hasParts = parts.some((part) => {
    if (part.type === 'reasoning') {
      return part.text.replaceAll('[REDACTED]', '').trim().length > 0;
    }
    if (part.type === 'compaction') {
      return part.text.trim().length > 0;
    }
    if (part.type === 'text') {
      return part.text.trim().length > 0;
    }
    return true;
  });

  if (!hasParts) return null;

  return (
    <SessionSectionCard>
      <div className={`flex flex-col ${sessionTheme.sectionGap}`}>
        {parts.map((part, index) =>
          renderPart(part, index, parts, isComplete),
        )}
        {message.error?.data?.message && (
          <p className={`${sessionTheme.fontMono} ${sessionTheme.error}`}>
            {message.error.data.message}
          </p>
        )}
      </div>
    </SessionSectionCard>
  );
}
