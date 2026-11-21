import { TicketMarkdown } from '../../components/TicketMarkdown';
import type { DesignSet } from '../../lib/schemas/design';
import { InlineDesignPicker } from '../designs/InlineDesignPicker';
import { pickerDomId } from '../designs/DesignPlanCard';
import { livePlanDesigns } from '../designs/useDesigns';
import { splitPlanMarkers } from './designMarkers';

/** Plan markdown with `{{design:<id>}}` replaced by inline pickers; unmarked sets at top. */
export function PlanBodyWithDesigns({
  plan,
  sets,
  ticketId,
}: {
  plan: string;
  sets: DesignSet[];
  ticketId: string;
}) {
  const byId = new Map(sets.map((set) => [set.id, set]));
  const segments = splitPlanMarkers(plan);
  const markedIds = new Set(
    segments.filter((seg): seg is { type: 'design'; setId: string } => seg.type === 'design')
      .map((seg) => seg.setId),
  );
  const fallback = livePlanDesigns(sets).filter((set) => !markedIds.has(set.id));

  return (
    <div className="space-y-2">
      {fallback.map((set) => (
        <InlineDesignPicker
          key={`fallback-${set.id}`}
          design={set}
          ticketId={ticketId}
          sectionId={pickerDomId(set.id)}
        />
      ))}
      {segments.map((segment, index) => {
        if (segment.type === 'text') {
          const text = segment.text.trim();
          if (!text) return null;
          return <TicketMarkdown key={`text-${index}`}>{segment.text}</TicketMarkdown>;
        }
        const design = byId.get(segment.setId);
        if (!design || design.superseded || design.inherited) {
          return null;
        }
        return (
          <InlineDesignPicker
            key={`mark-${segment.setId}-${index}`}
            design={design}
            ticketId={ticketId}
            sectionId={pickerDomId(segment.setId)}
          />
        );
      })}
    </div>
  );
}
