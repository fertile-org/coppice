import type { DesignSet } from '../../lib/schemas/design';

/** Kinds that render as structured cards (not HTML previews). */
export function isTechnicalDesignKind(
  kind: DesignSet['kind'],
): kind is 'architecture' | 'module' | 'dependency' {
  return kind === 'architecture' || kind === 'module' || kind === 'dependency';
}
