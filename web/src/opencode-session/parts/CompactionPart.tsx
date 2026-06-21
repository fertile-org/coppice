import { Badge } from '../../components/ui/badge';
import { CollapsibleDetail, excerptPreview } from '../components/CollapsibleDetail';
import { MarkdownContent } from '../components/MarkdownContent';
import type { CompactionPart as CompactionPartType } from '../sync/types';
import { sessionTheme } from '../theme/session-theme';
import { useTypewriter } from '../../features/runs/useTypewriter';

export function CompactionPart({
  part,
  streaming = false,
}: {
  part: CompactionPartType;
  streaming?: boolean;
}) {
  const content = part.text.trim();
  const revealed = useTypewriter(content, { enabled: streaming });
  if (!content) return null;

  return (
    <CollapsibleDetail
      label="Context compacted"
      preview={excerptPreview(content)}
      badge={
        part.auto ? (
          <Badge variant="outline" className="align-middle text-[10px]">
            auto
          </Badge>
        ) : undefined
      }
    >
      <div className={sessionTheme.fontMonoSm}>
        <MarkdownContent tone="thinking">{revealed}</MarkdownContent>
      </div>
    </CollapsibleDetail>
  );
}
