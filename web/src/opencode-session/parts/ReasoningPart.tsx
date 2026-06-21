import { CollapsibleDetail, excerptPreview } from '../components/CollapsibleDetail';
import { MarkdownContent } from '../components/MarkdownContent';
import type { ReasoningPart as ReasoningPartType } from '../sync/types';
import { sessionTheme } from '../theme/session-theme';
import { useTypewriter } from '../../features/runs/useTypewriter';

export function ReasoningPart({
  part,
  streaming = false,
}: {
  part: ReasoningPartType;
  streaming?: boolean;
}) {
  const content = part.text.replaceAll('[REDACTED]', '').trim();
  const revealed = useTypewriter(content, { enabled: streaming });
  if (!content) return null;

  return (
    <CollapsibleDetail
      label="Thinking"
      preview={excerptPreview(content)}
      streaming={streaming}
    >
      <div className={sessionTheme.fontMonoSm}>
        <MarkdownContent tone="thinking">{revealed}</MarkdownContent>
      </div>
    </CollapsibleDetail>
  );
}
