import { Badge } from '../../components/ui/badge';
import { CollapsibleDetail, excerptPreview } from '../components/CollapsibleDetail';
import { MarkdownContent } from '../components/MarkdownContent';
import type { CompactionPart as CompactionPartType } from '../sync/types';
import { sessionTheme } from '../theme/session-theme';

export function CompactionPart({ part }: { part: CompactionPartType }) {
  const content = part.text.trim();
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
        <MarkdownContent tone="thinking">{content}</MarkdownContent>
      </div>
    </CollapsibleDetail>
  );
}
