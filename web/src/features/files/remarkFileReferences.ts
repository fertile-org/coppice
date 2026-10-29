import { findProseFileReferences } from './fileReference';

/** Link URL scheme the markdown renderer turns into a file preview button. */
export const FILE_REFERENCE_URL_PREFIX = 'coppice-file:';

interface MdNode {
  type: string;
  value?: string;
  url?: string;
  children?: MdNode[];
}

// Text under these already has its own target (or is code, handled separately).
const SKIP_PARENTS = new Set(['link', 'linkReference', 'definition']);

function splitText(node: MdNode): MdNode[] {
  const value = node.value ?? '';
  const matches = findProseFileReferences(value);
  if (matches.length === 0) return [node];
  const out: MdNode[] = [];
  let cursor = 0;
  for (const match of matches) {
    if (match.start > cursor) out.push({ type: 'text', value: value.slice(cursor, match.start) });
    out.push({
      type: 'link',
      url: FILE_REFERENCE_URL_PREFIX + encodeURIComponent(match.raw),
      children: [{ type: 'text', value: match.raw }],
    });
    cursor = match.end;
  }
  if (cursor < value.length) out.push({ type: 'text', value: value.slice(cursor) });
  return out;
}

function visit(node: MdNode) {
  if (!node.children || SKIP_PARENTS.has(node.type)) return;
  node.children = node.children.flatMap((child) => {
    if (child.type === 'text') return splitText(child);
    visit(child);
    return [child];
  });
}

/** Remark plugin: turns repo file paths in prose into file reference links. */
export function remarkFileReferences() {
  return (tree: MdNode) => visit(tree);
}
