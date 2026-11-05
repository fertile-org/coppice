import { markEdits, tokenize, type HunkData } from 'react-diff-view';
import { hljs } from '../../lib/highlight';
import { highlightLanguage } from '../files/fileReference';

type HighlightNode =
  | { type: 'text'; value: string }
  | {
      type: 'element';
      properties: { className: string[] };
      children: HighlightNode[];
    };

function nodesFromElement(element: Element): HighlightNode[] {
  const nodes: HighlightNode[] = [];
  for (const child of element.childNodes) {
    if (child.nodeType === Node.TEXT_NODE) {
      nodes.push({ type: 'text', value: child.textContent ?? '' });
    } else if (child instanceof Element) {
      nodes.push({
        type: 'element',
        properties: { className: [...child.classList] },
        children: nodesFromElement(child),
      });
    }
  }
  return nodes;
}

/**
 * react-diff-view asks for refractor nodes. File previews already use
 * highlight.js, so this turns that HTML into the node list instead of
 * adding a second highlighter.
 */
const highlightRefractor = {
  highlight(text: string, language: string): HighlightNode[] {
    if (!hljs.getLanguage(language)) {
      return [{ type: 'text', value: text }];
    }
    const html = hljs.highlight(text, { language, ignoreIllegals: true }).value;
    const root = new DOMParser().parseFromString(`<div>${html}</div>`, 'text/html').body
      .firstElementChild;
    return root ? nodesFromElement(root) : [{ type: 'text', value: text }];
  },
};

export function diffTokens(hunks: HunkData[], path: string | undefined) {
  const language = path ? highlightLanguage(path) : undefined;
  const enhancers = [markEdits(hunks)];
  if (!language) return tokenize(hunks, { enhancers });
  return tokenize(hunks, {
    highlight: true,
    refractor: highlightRefractor,
    language,
    enhancers,
  });
}
