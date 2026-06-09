import { CollapsibleDetail, excerptPreview } from '../components/CollapsibleDetail';
import { MarkdownContent } from '../components/MarkdownContent';
import type { ReasoningPart as ReasoningPartType } from '../sync/types';
import { sessionTheme } from '../theme/session-theme';

export function ReasoningPart({
  part,
  streaming = false,
}: {
  part: ReasoningPartType;
  streaming?: boolean;
}) {
  const content = part.text.replaceAll('[REDACTED]', '').trim();
  if (!content) return null;

  return (
    <CollapsibleDetail
      label="Thinking"
      preview={excerptPreview(content)}
      streaming={streaming}
    >
      <div className={sessionTheme.fontMonoSm}>
        <MarkdownContent tone="thinking">{content}</MarkdownContent>
      </div>
    </CollapsibleDetail>
  );
}
