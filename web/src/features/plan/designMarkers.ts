/** Same acceptance as server `design_markers`: UUID or 12-char `[0-9a-z]`. */
const DESIGN_MARKER =
  /\{\{design:([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}|[0-9a-z]{12})\}\}/g;

export type PlanSegment =
  | { type: 'text'; text: string }
  | { type: 'design'; setId: string };

/** Split plan markdown at design markers. Unknown ids stay as empty design segments. */
export function splitPlanMarkers(markdown: string): PlanSegment[] {
  const segments: PlanSegment[] = [];
  let last = 0;
  const re = new RegExp(DESIGN_MARKER.source, 'g');
  for (const match of markdown.matchAll(re)) {
    const index = match.index ?? 0;
    if (index > last) {
      segments.push({ type: 'text', text: markdown.slice(last, index) });
    }
    segments.push({ type: 'design', setId: match[1]! });
    last = index + match[0].length;
  }
  if (last < markdown.length) {
    segments.push({ type: 'text', text: markdown.slice(last) });
  }
  return segments;
}

/** Remove markers so approved description / history never show raw `{{design:…}}`. */
export function stripDesignMarkers(markdown: string): string {
  return markdown
    .replace(DESIGN_MARKER, '')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

/** Nearest preceding `##` heading above the first marker for `setId`. */
export function sectionForSet(markdown: string, setId: string): string | null {
  const needle = `{{design:${setId}}}`;
  const pos = markdown.indexOf(needle);
  if (pos < 0) return null;
  let heading: string | null = null;
  for (const line of markdown.slice(0, pos).split('\n')) {
    const trimmed = line.trim();
    if (trimmed.startsWith('## ') && !trimmed.startsWith('###')) {
      heading = trimmed.slice(3).trim();
    }
  }
  return heading;
}
